use std::cell::RefCell;
use std::collections::vec_deque::VecDeque;
use std::collections::HashMap;
use std::mem::size_of;
use std::path::Path;
use std::process;
use std::string::String;

use tock_cells::map_cell::MapCell;

use host_emulation::common_types;

use kernel::AppId;
use kernel::AppSlice;
use kernel::Chip;
use kernel::Kernel;
use kernel::ReturnCode;
use kernel::Shared;
use kernel::capabilities::ExternalProcessCapability;
use kernel::debug;
use kernel::mpu;
use kernel::CallbackId;
use kernel::syscall::{self, Syscall, UserspaceKernelBoundary};
use kernel::procs::{State, Task, FunctionCallSource, FunctionCall,
    ProcessType, Error};

use core::cell::Cell;
use core::fmt::Write;
use core::ptr::NonNull;

use crate::syscall_transport::SyscallTransport;
use crate::Result;

pub struct UnixProcess<'a> {
    id: usize,
    _name: String,
    proc_path: &'a Path,
    process: MapCell<process::Child>,
    allow_map: RefCell<HashMap<*const u8, AllowSlice>>,
}

struct AllowSlice {
    slice: Vec<u8>,
    valid: bool,
}

impl AllowSlice {
    fn new(slice: Vec<u8>) -> AllowSlice {
        AllowSlice {
            slice: slice,
            valid: false,
        }
    }

    fn is_valid(&self) -> bool {
        self.valid
    }

    fn len(&self) -> usize {
        self.slice.len()
    }

    fn get(&self) -> &Vec<u8> {
        &self.slice
    }

    fn get_mut(&mut self) -> &mut Vec<u8> {
        &mut self.slice
    }

    fn validate(&mut self) {
        self.valid = true;
    }
}

impl<'a> UnixProcess<'a> {
    pub fn new(exec: &'a Path, name: String, id: usize) -> UnixProcess<'a> {
        UnixProcess {
            id: id,
            _name: name,
            proc_path: exec,
            process: MapCell::empty(),
            allow_map: RefCell::new(HashMap::new()),
        }
    }

    /// Starts the process and supplies necessary command line arguments.
    pub fn start(&self, socket_rx: &Path, socket_tx: &Path) -> Result<()> {
        debug!("Starting process {}", self.proc_path.to_str().unwrap_or_default());
        let proc = process::Command::new(self.proc_path)
            .arg("--id")
            .arg(self.id.to_string())
            .arg("--socket_send")
            .arg(socket_rx)
            .arg("--socket_recv")
            .arg(socket_tx)
            .spawn()?;
        self.process.put(proc);
        Ok(())
    }

    /// Checks if the process has yet been started.
    pub fn was_started(&self) -> bool {
        self.process.is_some()
    }

    /// Adds a slice to `allow_map` and translates the address from app memory
    /// space to kernel memory space.
    pub fn allow(&self, app_slice_addr: *const u8, len: usize) -> *mut u8 {
        if app_slice_addr.is_null() {
            return app_slice_addr as *mut u8;
        }

        let mut allow_map = self.allow_map.borrow_mut();
        let mut slice = vec![0 as u8; len];
        let ret = slice.as_mut_ptr();
        allow_map.insert(app_slice_addr, AllowSlice::new(slice));

        ret
    }

    /// Passes livliness to the process.
    /// 1. Send return value from previous syscall, unless
    ///     a) We are issuing a callback, then send the pc and args.
    ///     b) The process just started, then send nothing.
    ///    In this same payload, include the number of allowed regions.
    /// 2. Send all allowed slices back to the app.
    /// 3. Block on reading the syscall socket, waiting for the next syscall.
    /// 4. Read all allowed slices from the app.
    ///
    /// Note that if we get an allow syscall here we don't actually copy the
    /// allowed buffer over from the app on this syscall, but on the next one.
    /// This should be okay, as nothing should read from that slice until
    /// `command` is called, at which point that slice will be copied from the
    /// app.
    pub fn unyield(
        &self,
        transport: &SyscallTransport,
        syscall_return: Option<common_types::KernelReturn>,
    ) -> Result<common_types::Syscall> {
        if let Some(ret) = syscall_return {
            transport.tx_connect_if_needed()?;
            transport.send(self.get_id(), &ret)?;
            self.transfer_allow_region(transport, false)?;
        }

        let mut buf: [u8; size_of::<common_types::Syscall>()] =
            [0; size_of::<common_types::Syscall>()];
        let syscall = transport.recv(&mut buf)?;

        transport.tx_connect_if_needed()?;
        self.transfer_allow_region(transport, true)?;

        Ok(*syscall)
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    /// Iterates through all known shared slices and, depending on the value of
    /// `send`, either transfers the contents of each slice to the app or
    /// requests each slice to be sent from the app. In either case a sequence
    /// of zero or more `AllowedRegionPreamble` structs with non-null `addr`
    /// fields are sent to the app, one for each allowed region. Once all
    /// preambles have been sent a final `AllowedRegionPreamble` with a null
    /// `addr` field is sent to signal to the app that all slices have been
    /// transfered.
    ///
    /// Both the app and the kernel know whether they should be sending or
    /// receiving slices depending on which of the two is currently live. When
    /// making a syscall it is the app's turn to send over the contents of all
    /// allowed buffers, and when returning from a syscall or issuing a callback
    /// it is the kernel's turn to send over the contents of all allowed
    /// buffers.
    ///
    /// In the special case that the kernel is returning from an "allow" syscall
    /// the new slice that was just created will not yet be populated with the
    /// proper data. A call to `AllowSlice::is_valid()` ensures that each slice
    /// has been received from the app at least once before transferring it back
    /// to avoid clobbering valid data on the app side.
    ///
    /// The apps do not maintain any information about what slices they have
    /// allowed to the kernel. It is the kernel's sole responsibility to track
    /// that information and only request valid slices from the apps.
    fn transfer_allow_region(&self, transport: &SyscallTransport, send: bool) -> Result<()> {
        let mut allow_map = self.allow_map.borrow_mut();
        for (addr, slice) in allow_map.iter_mut() {
            // We need to be careful not to send slices that have not yet been
            // recieved from the app. Writing an invalid slice back to the
            // process can insert invalid data.
            if send && !slice.is_valid() {
                continue;
            }

            let preamble = common_types::AllowedRegionPreamble::new(*addr as usize, slice.len());
            transport.send(self.get_id(), &preamble)?;

            if send {
                transport.send_bytes(self.get_id(), slice.get())?;
            } else {
                transport.recv_bytes(slice.get_mut())?;
                slice.validate();
            }
        }

        // The app doesn't know how many of these messages to expect, so write
        // this special "null terminator" to indicate that we are done.
        let null_terminator = common_types::AllowedRegionPreamble::new_null();
        transport.send(self.get_id(), &null_terminator)?;

        Ok(())
    }
}

#[derive(Default)]
struct ProcessDebug {
    timeslice_expiration_count: usize,
    dropped_callback_count: usize,
    syscall_count: usize,
    last_syscall: Option<Syscall>,
}

pub struct EmulatedProcess<C: 'static + Chip> {
    app_id: Cell<AppId>,
    name: &'static str,
    chip: &'static C,
    kernel: &'static Kernel,
    state: Cell<State>,
    tasks: MapCell<VecDeque<Task>>,
    stored_state:
        MapCell<&'static mut <<C as Chip>::UserspaceKernelBoundary as UserspaceKernelBoundary>::StoredState>,
    grant_region: Cell<*mut *mut u8>,
    restart_count: Cell<usize>,
    debug: MapCell<ProcessDebug>,
    external_process_cap: &'static dyn ExternalProcessCapability,
}

impl<C: 'static + Chip> EmulatedProcess<C> {
    pub fn create(
        app_id: AppId,
        name: &'static str,
        chip: &'static C,
        kernel: &'static Kernel,
        start_state: &'static mut <<C as Chip>::UserspaceKernelBoundary as UserspaceKernelBoundary>::StoredState,
        external_process_cap: &'static dyn ExternalProcessCapability,
    ) -> core::result::Result<EmulatedProcess<C>, ()> {
        let process = EmulatedProcess {
            app_id: Cell::new(app_id),
            name: name,
            chip: chip,
            kernel: kernel,
            state: Cell::new(State::Unstarted),
            tasks: MapCell::new(VecDeque::with_capacity(10)),
            stored_state: MapCell::new(start_state),
            grant_region: Cell::new(0 as *mut *mut u8),
            restart_count: Cell::new(0),
            debug: MapCell::new(ProcessDebug::default()),
            external_process_cap: external_process_cap,
        };

        let _ = process.stored_state.map(|stored_state| {
            unsafe {
                chip.userspace_kernel_boundary().initialize_process(
                    0 as *const usize,
                    0,
                    stored_state)
            }
        }).ok_or(())?;

        // Use a special pc of 0 to indicate that we need to exec the process.
        process.tasks.map(|tasks| {
            tasks.push_back(Task::FunctionCall(FunctionCall {
                source: FunctionCallSource::Kernel,
                pc: 0,
                argument0: 0,
                argument1: 0,
                argument2: 0,
                argument3: 0,
            }));
        });

        kernel.increment_work_external(external_process_cap);
        Ok(process)
    }
}

impl<C: 'static + Chip> EmulatedProcess<C> {
    fn is_active(&self) -> bool {
        let state = self.state.get();
        state != State::StoppedFaulted && state != State::Fault
    }
}

impl<C: 'static + Chip> ProcessType for EmulatedProcess<C> {
    fn appid(&self) -> AppId {
        self.app_id.get()
    }

    fn enqueue_task(&self, task: Task) -> bool {
        if !self.is_active() {
            return false;
        }

        self.kernel.increment_work_external(self.external_process_cap);

        let ret = self.tasks.map_or(false, |tasks| {
            tasks.push_back(task);
            true
        });

        if !ret {
            self.debug.map(|debug| {
                debug.dropped_callback_count += 1;
            });
        }

        ret
    }

    fn dequeue_task(&self) -> Option<Task> {
        self.tasks.map_or(None, |tasks| {
            tasks.pop_front().map(|cb| {
                self.kernel.decrement_work_external(self.external_process_cap);
                cb
            })
        })
    }

    fn remove_pending_callbacks(&self, callback_id: CallbackId) {
        self.tasks.map(|tasks| {
            tasks.retain(|task| match task {
                Task::FunctionCall(call) => match call.source {
                    FunctionCallSource::Kernel => true,
                    FunctionCallSource::Driver(id) => id != callback_id,
                },
                _ => true,
            })
        });
    }

    fn get_state(&self) -> State {
        self.state.get()
    }

    fn set_yielded_state(&self) {
        if self.state.get() == State::Running {
            self.state.set(State::Yielded);
            self.kernel.decrement_work_external(self.external_process_cap);
        }
    }

    fn stop(&self) {
        match self.state.get() {
            State::Running => self.state.set(State::StoppedRunning),
            State::Yielded => self.state.set(State::StoppedYielded),
            _ => {}
        }
    }

    fn resume(&self) {
        match self.state.get() {
            State::StoppedRunning => self.state.set(State::Running),
            State::StoppedYielded => self.state.set(State::Yielded),
            _ => {}
        }
    }

    fn set_fault_state(&self) {
        self.state.set(State::Fault);

        // TODO Handle based on `FaultResponse`
        panic!("Process {} has a fault", self.get_process_name());
    }

    fn get_restart_count(&self) -> usize {
        self.restart_count.get()
    }

    fn get_process_name(&self) -> &'static str {
        self.name
    }

    fn allow(
        &self,
        buf_start_addr: *const u8,
        size: usize,
    ) -> core::result::Result<Option<AppSlice<Shared, u8>>, ReturnCode> {
        // The work has already been done, and |buf_start_addr| ponts to a
        // buffer in this process's heap. We just need to manipulate types here.

        match NonNull::new(buf_start_addr as *mut u8) {
            None => Ok(None),
            Some(buf_start) => {
                let slice = unsafe {
                    AppSlice::new_external(
                        buf_start, size, self.appid(), self.external_process_cap)
                };
                Ok(Some(slice))
            }
        }
    }

    fn flash_non_protected_start(&self) -> *const u8 {
        0 as *const u8
    }

    fn setup_mpu(&self) {}

    fn add_mpu_region(
        &self,
        _unallocated_memory_start: *const u8,
        _unallocated_memory_size: usize,
        _min_region_size: usize,
    ) -> Option<mpu::Region> {
        None
    }

    fn alloc(&self, size: usize, align: usize) -> Option<NonNull<u8>> {
        if !self.is_active() {
            return None;
        }
        let layout = match std::alloc::Layout::from_size_align(size, align) {
            Ok(l) => l,
            Err(e) => {
                debug!("Failed to alloc region: {}", e);
                return None;
            }
        };
        unsafe {
            let region = std::alloc::alloc(layout);
            Some(NonNull::new_unchecked(region as *mut u8))
        }
    }

    unsafe fn free(&self, _: *mut u8) {
        // Tock processes don't support free yet.
    }

    fn get_grant_ptr(&self, grant_num: usize) -> Option<*mut u8> {
        if !self.is_active() {
            return None;
        }

        if grant_num >= self.kernel.get_grant_count_and_finalize_external(
            self.external_process_cap) {
            return None;
        }

        Some(self.grant_region.get() as *mut u8)
    }

    unsafe fn set_grant_ptr(&self, _grant_num: usize, grant_ptr: *mut u8) {
        self.grant_region.set(grant_ptr as *mut *mut u8);
    }

    unsafe fn set_syscall_return_value(&self, return_value: isize) {
        self.stored_state.map(|stored_state| {
            self.chip
                .userspace_kernel_boundary()
                .set_syscall_return_value(0 as *const usize, stored_state, return_value);
        });
    }

    unsafe fn set_process_function(&self, callback: FunctionCall) {
        let res = self.stored_state.map(|stored_state| {
            self.chip.userspace_kernel_boundary().set_process_function(
                0 as *const usize,
                0,
                stored_state,
                callback,
            )
        });

        match res {
            Some(Ok(_)) => {
                self.kernel.increment_work_external(self.external_process_cap);
                self.state.set(State::Running);
            }

            None | Some(Err(_)) => {
                self.set_fault_state();
            }
        }
    }

    unsafe fn switch_to(&self) -> Option<syscall::ContextSwitchReason> {
        let res = self.stored_state.map(|stored_state| {
            self.chip.userspace_kernel_boundary()
                .switch_to_process(0 as *const usize, stored_state).1
        })?;

        self.debug.map(|debug| {
            if res == syscall::ContextSwitchReason::TimesliceExpired {
                debug.timeslice_expiration_count += 1;
            }
        });

        Some(res)
    }

    fn debug_syscall_count(&self) -> usize {
        self.debug.map_or(0, |debug| debug.syscall_count)
    }

    fn debug_dropped_callback_count(&self) -> usize {
        self.debug.map_or(0, |debug| debug.dropped_callback_count)
    }

    fn debug_timeslice_expiration_count(&self) -> usize {
        self.debug
            .map_or(0, |debug| debug.timeslice_expiration_count)
    }

    fn debug_timeslice_expired(&self) {
        self.debug
            .map(|debug| debug.timeslice_expiration_count += 1);
    }

    fn debug_syscall_called(&self, last_syscall: Syscall) {
        self.debug.map(|debug| {
            debug.syscall_count += 1;
            debug.last_syscall = Some(last_syscall);
        });
    }

    // *************************************************************************
    // Functions bellow are required by the `ProcessType` trait but are either
    // unused or do not translate to this framework and are treated as NO-OPs.
    // *************************************************************************
    unsafe fn print_memory_map(&self, _writer: &mut dyn Write) {}

    unsafe fn print_full_process(&self, _writer: &mut dyn Write) {}

    fn brk(&self, _new_break: *const u8) -> core::result::Result<*const u8, Error> {
        Ok(0 as *const u8)
    }

    fn sbrk(&self, _increment: isize) -> core::result::Result<*const u8, Error> {
        Ok(0 as *const u8)
    }

    fn mem_start(&self) -> *const u8 {
        0 as *const u8
    }

    fn mem_end(&self) -> *const u8 {
        0 as *const u8
    }

    fn flash_start(&self) -> *const u8 {
        0 as *const u8
    }

    fn flash_end(&self) -> *const u8 {
        0 as *const u8
    }

    fn kernel_memory_break(&self) -> *const u8 {
        0 as *const u8
    }

    fn number_writeable_flash_regions(&self) -> usize {
        0
    }

    fn get_writeable_flash_region(&self, _region_index: usize) -> (u32, u32) {
        (0, 0)
    }

    fn update_stack_start_pointer(&self, _stack_pointer: *const u8) {}

    fn update_heap_start_pointer(&self, _heap_pointer: *const u8) {}
}

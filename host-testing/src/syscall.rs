use core::fmt::Write;

use kernel;
use kernel::debug;
use kernel::syscall::ContextSwitchReason;

use crate::process::UnixProcess;
use crate::syscall_transport::SyscallTransport;
use crate::Result;

use host_emulation::common_types;

use std::path::PathBuf;

#[repr(packed)]
#[allow(unused)]
#[derive(Default)]
pub struct SysCallArgs {
    identifier: usize,
    syscall_number: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
}

#[derive(Default, Copy, Clone)]
pub struct HostStoredState {
    process: Option<&'static UnixProcess<'static>>,
    syscall_ret: common_types::KernelReturn,
}

pub struct SysCall {
    transport: SyscallTransport,
}

impl HostStoredState {
    pub fn new(process: &'static UnixProcess<'static>) -> Self {
        HostStoredState {
            process: Some(process),
            ..Default::default()
        }
    }
}

impl SysCall {
    pub fn try_new(syscall_rx: PathBuf, syscall_tx: PathBuf) -> Result<SysCall> {
        Ok(SysCall {
            transport: SyscallTransport::open(syscall_tx, syscall_rx)?,
        })
    }

    pub fn get_transport(&self) -> &SyscallTransport {
        &self.transport
    }
}

impl kernel::syscall::UserspaceKernelBoundary for SysCall {
    type StoredState = HostStoredState;

    unsafe fn initialize_process(
        &self,
        stack_pointer: *const usize,
        _stack_size: usize,
        _state: &mut Self::StoredState,
    ) -> core::result::Result<*const usize, ()> {
        // Do nothing as Unix process will be started on first switch_to_process
        // This is good place for synchronize libtock-rs startup
        // Right now this is not needed b/c libtock-rs not require
        // antyhing special right now
        Ok(stack_pointer as *mut usize)
    }

    unsafe fn set_syscall_return_value(
        &self,
        _stack_pointer: *const usize,
        state: &mut Self::StoredState,
        return_value: isize,
    ) {
        state.syscall_ret = common_types::KernelReturn::new_ret(return_value);
    }

    unsafe fn set_process_function(
        &self,
        stack_pointer: *const usize,
        _remaining_stack_memory: usize,
        state: &mut Self::StoredState,
        callback: kernel::procs::FunctionCall,
    ) -> core::result::Result<*mut usize, *mut usize> {
        // Do nothing as Unix process will be started on first
        // switch_to_process
        state.syscall_ret = common_types::KernelReturn::new_cb(common_types::Callback::new(
            callback.pc,
            callback.argument0,
            callback.argument1,
            callback.argument2,
            callback.argument3,
        ));

        Ok(stack_pointer as *mut usize)
    }

    unsafe fn switch_to_process(
        &self,
        stack_pointer: *const usize,
        state: &mut Self::StoredState,
    ) -> (*mut usize, ContextSwitchReason) {
        debug!("Switch");
        let process = match state.process {
            Some(p) => p,
            None => return (stack_pointer as *mut usize, ContextSwitchReason::Fault),
        };

        let return_value: Option<common_types::KernelReturn>;

        // Start application process here when, as this is first
        // function called by scheduler when switching to app process
        // for first time
        if !process.was_started() {
            let transport = self.get_transport();

            if let Err(e) = process.start(transport.rx_path(), transport.tx_path()) {
                debug!("Failed to start process {}", e);
                return (stack_pointer as *mut usize, ContextSwitchReason::Fault);
            }
            return_value = None;
        } else {
            return_value = Some(state.syscall_ret);
        }

        // This is where we stop execution and wait for the process to yield.
        let syscall_args = match process.unyield(&self.get_transport(), return_value) {
            Ok(syscall) => syscall,
            Err(e) => {
                debug!("Failed to resume process: {}", e);
                return (stack_pointer as *mut usize, ContextSwitchReason::Fault);
            }
        };

        debug!("{:?}", syscall_args);
        let syscall = kernel::syscall::arguments_to_syscall(
            syscall_args.syscall_number as u8,
            syscall_args.args[0],
            syscall_args.args[1],
            syscall_args.args[2],
            syscall_args.args[3],
        );

        let ret = match syscall {
            Some(mut s) => {
                // For scoping reasons we need to handle ALLOW here. When we're
                // done we get a new pointer that we pass back to the kernel.
                if let kernel::syscall::Syscall::ALLOW {
                    driver_number,
                    subdriver_number,
                    mut allow_address,
                    allow_size,
                } = s
                {
                    // Translate allow region address to kernel memory.
                    debug!("Translating allow region.");
                    allow_address = process.allow(allow_address as *const u8, allow_size);

                    s = kernel::syscall::Syscall::ALLOW {
                        driver_number: driver_number,
                        subdriver_number: subdriver_number,
                        allow_address: allow_address,
                        allow_size: allow_size,
                    };
                }
                ContextSwitchReason::SyscallFired { syscall: s }
            }
            None => ContextSwitchReason::Fault,
        };
        (stack_pointer as *mut usize, ret)
    }

    unsafe fn print_context(
        &self,
        _stack_pointer: *const usize,
        _state: &Self::StoredState,
        _writer: &mut dyn Write,
    ) {
    }
}

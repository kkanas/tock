use std::io;
use std::thread;

use kernel::capabilities;
use kernel::common::dynamic_deferred_call::{DynamicDeferredCall, DynamicDeferredCallClientState};
use kernel::component::Component;
use kernel::AppId;
use kernel::Platform;
use kernel::{create_capability, static_init};
use kernel::debug;

mod chip;
mod interrupt;
mod process;
mod syscall;
mod syscall_transport;
mod systick;
mod test_client;
mod uart;

use crate::interrupt::{new_interrupt_channel, LowerHalf, UpperHalf};
use crate::process::{EmulatedProcess, UnixProcess};
use crate::syscall::HostStoredState;
use crate::test_client::TestClient;

pub type Result<T> = std::result::Result<T, EmulationError>;

#[derive(Debug)]
pub enum EmulationError {
    IoError(io::Error),
    ChannelError,
    PartialMessage(usize, usize),
    Custom(String),
}

impl From<io::Error> for EmulationError {
    fn from(error: io::Error) -> Self {
        EmulationError::IoError(error)
    }
}

impl std::fmt::Display for EmulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmulationError::IoError(e) => write!(f, "{}", e),
            EmulationError::ChannelError => write!(f, "Channel Error"),
            EmulationError::PartialMessage(e, a) => {
                write!(f, "Unexpected message length. Expected {}, got {}.", e, a)
            }
            EmulationError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

static mut UNINITIALIZED_PROCESSES: [Option<&'static UnixProcess>; 4] =
    [None, None, None, None];

static mut PROCESSES: [Option<&'static dyn kernel::procs::ProcessType>; 4] =
    [None, None, None, None];

static mut CHIP: Option<&'static chip::HostChip> = None;

static mut EXTERNAL_PROCESS_CAP: &dyn capabilities::ExternalProcessCapability =
    &create_capability!(capabilities::ExternalProcessCapability);

/// A structure representing this platform that holds references to all
/// capsules for this platform.
struct HostBoard {
    console: &'static capsules::console::Console<'static>,
    lldb: &'static capsules::low_level_debug::LowLevelDebug<
        'static,
        capsules::virtual_uart::UartDevice<'static>,
    >,
}

/// Mapping of integer syscalls to objects that implement syscalls.
impl Platform for HostBoard {
    fn with_driver<F, R>(&self, driver_num: usize, f: F) -> R
    where
        F: FnOnce(Option<&dyn kernel::Driver>) -> R,
    {
        match driver_num {
            capsules::console::DRIVER_NUM => f(Some(self.console)),
            capsules::low_level_debug::DRIVER_NUM => f(Some(self.lldb)),
            _ => f(None),
        }
    }
}

fn serve_external_interrupts(irq_source: UpperHalf) {
    thread::spawn(move || {
        if let Err(e) = irq_source.spin() {
            panic!("Interrupt source stopped {}", e);
        }
    });
}

static mut TX_DATA: &'static mut [u8; 128] = &mut [0; 128];
#[no_mangle]
pub unsafe fn reset_handler() {
    let main_loop_cap = create_capability!(capabilities::MainLoopCapability);
    let board_kernel = static_init!(kernel::Kernel, kernel::Kernel::new(&PROCESSES));

    let chip = CHIP.unwrap();

    let dynamic_deferred_call_clients =
        static_init!([DynamicDeferredCallClientState; 1], Default::default());
    let dynamic_deferred_caller = static_init!(
        DynamicDeferredCall,
        DynamicDeferredCall::new(dynamic_deferred_call_clients)
    );
    DynamicDeferredCall::set_global_instance(dynamic_deferred_caller);

    let stdin = static_init!(io::Stdin, io::stdin());
    let stdout = static_init!(io::Stdout, io::stdout());
    let uart = static_init!(
        uart::UartIO, uart::UartIO::new(stdin, stdout, dynamic_deferred_caller));

    let uart_mux =
        components::console::UartMuxComponent::new(uart, 0, dynamic_deferred_caller).finalize(());

    {
        // Uart won't work before console mux is initalized
        // because uses tx_client is OptionalCell::empty()
        // until console::UartMuxComponent::new calls set_transmit_client
        let tx_str  = "Hello Test\n";
        let tx_len = tx_str.len();
        (&mut TX_DATA[..tx_len]).copy_from_slice(&tx_str.as_bytes()[..tx_len]);
        use kernel::hil::uart::Transmit;
        uart.transmit_buffer(TX_DATA, tx_len);
    }

    let console = components::console::ConsoleComponent::new(board_kernel, uart_mux).finalize(());

    components::debug_writer::DebugWriterNoMuxComponent::new(uart).finalize(());

    let lldb = components::lldb::LowLevelDebugComponent::new(board_kernel, uart_mux).finalize(());

    let host = HostBoard {
        console: console,
        lldb: lldb,
    };

    // Process setup. This takes the place of TBF headers
    for i in 0..UNINITIALIZED_PROCESSES.len() {
        let uninitialized_process = match UNINITIALIZED_PROCESSES[i] {
            Some(p) => p,
            None => break,
        };
        let state = static_init!(HostStoredState, HostStoredState::new(uninitialized_process));
        match EmulatedProcess::<chip::HostChip>::create(
            AppId::new_external(board_kernel, i, i, EXTERNAL_PROCESS_CAP),
            "Sample Process",
            chip,
            board_kernel,
            state,
            EXTERNAL_PROCESS_CAP,
        ) {
            Ok(p) => PROCESSES[i] = Some(static_init!(process::EmulatedProcess<chip::HostChip>, p)),
            Err(_) => debug!("Failed to start process #{}: ", i),
        }
    }

    board_kernel.kernel_loop(&host, chip, None, &main_loop_cap);
}

pub fn main() {
    unsafe {
        let config = static_init!(TestClient, TestClient::from_cmd_line_args().unwrap());
        let (irq_upper, irq_lower) = match new_interrupt_channel(&config.irq_path()) {
            Ok((upper, lower)) => (upper, lower),
            Err(e) => panic!("Failed to create irq handler: {}", e),
        };

        let irq_lower = static_init!(LowerHalf, irq_lower);
        serve_external_interrupts(irq_upper);

        let chip = static_init!(
            chip::HostChip,
            chip::HostChip::try_new(
                irq_lower,
                config.syscall_rx_path(),
                config.syscall_tx_path(),
            )
            .unwrap()
        );

        let apps = config.apps();
        assert_eq!(apps.len(), 1);

        let app_path = apps[0].bin_path();

        UNINITIALIZED_PROCESSES[0] = Some(static_init!(
            UnixProcess,
            UnixProcess::new(app_path, "Sample Process".to_string(), 0)
        ));
        CHIP = Some(chip);
        reset_handler();
    }
}

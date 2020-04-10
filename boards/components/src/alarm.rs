//! Components for hardware timer Alarms.
//!
//! This provides two components, `AlarmMuxComponent`, which provides a
//! multiplexed interface to a hardware alarm, and `AlarmDriverComponent`,
//! which provides an alarm system call interface.
//!
//! Usage
//! -----
//! ```rust
//! let ast = &sam4l::ast::AST;
//! let mux_alarm = components::alarm::AlarmMuxComponent::new(ast)
//!     .finalize(components::alarm_mux_component_helper!(sam4l::ast::Ast));
//! ast.configure(mux_alarm);
//! let alarm = components::alarm::AlarmDriverComponent::new(board_kernel, mux_alarm)
//!     .finalize(components::alarm_component_helper!(sam4l::ast::Ast));
//! ```

// Author: Philip Levis <pal@cs.stanford.edu>
// Last modified: 12/21/2019

use core::mem::MaybeUninit;

use capsules::alarm::AlarmDriver;
use capsules::virtual_alarm::{MuxAlarm, VirtualMuxAlarm};
use kernel::capabilities;
use kernel::component::Component;
use kernel::create_capability;
use kernel::hil::time;
use kernel::static_init_half;
use kernel::StaticUninitializedBuffer;

// Setup static space for the objects.
#[macro_export]
macro_rules! alarm_mux_component_buffer {
    ($A:ty) => {{
        use capsules::virtual_alarm::MuxAlarm;
        $crate::static_buf!(MuxAlarm<'static, $A>)
    };};
}

// Setup static space for the objects.
#[macro_export]
macro_rules! alarm_component_buffer {
    ($A:ty) => {{
        use capsules::alarm::AlarmDriver;
        use capsules::virtual_alarm::VirtualMuxAlarm;
        (
            $crate::static_buf!(VirtualMuxAlarm<'static, $A>),
            $crate::static_buf!(AlarmDriver<'static, VirtualMuxAlarm<'static, $A>>),
        )
    };};
}

pub struct AlarmMuxComponent<A: 'static + time::Alarm<'static>> {
    alarm: &'static A,
}

impl<A: 'static + time::Alarm<'static>> AlarmMuxComponent<A> {
    pub fn new(alarm: &'static A) -> AlarmMuxComponent<A> {
        AlarmMuxComponent { alarm }
    }
}

impl<A: 'static + time::Alarm<'static>> Component for AlarmMuxComponent<A> {
    type StaticInput = StaticUninitializedBuffer<MuxAlarm<'static, A>>;
    type Output = &'static MuxAlarm<'static, A>;

    unsafe fn finalize(self, static_buffer: Self::StaticInput) -> Self::Output {
        let mux_alarm = static_buffer.initialize(MuxAlarm::new(self.alarm));

        time::Alarm::set_client(self.alarm, mux_alarm);
        mux_alarm
    }
}

pub struct AlarmDriverComponent<A: 'static + time::Alarm<'static>> {
    board_kernel: &'static kernel::Kernel,
    alarm_mux: &'static MuxAlarm<'static, A>,
}

impl<A: 'static + time::Alarm<'static>> AlarmDriverComponent<A> {
    pub fn new(
        board_kernel: &'static kernel::Kernel,
        mux: &'static MuxAlarm<'static, A>,
    ) -> AlarmDriverComponent<A> {
        AlarmDriverComponent {
            board_kernel: board_kernel,
            alarm_mux: mux,
        }
    }
}

impl<A: 'static + time::Alarm<'static>> Component for AlarmDriverComponent<A> {
    type StaticInput = (
        StaticUninitializedBuffer<VirtualMuxAlarm<'static, A>>,
        StaticUninitializedBuffer<AlarmDriver<'static, VirtualMuxAlarm<'static, A>>>,
    );
    type Output = &'static AlarmDriver<'static, VirtualMuxAlarm<'static, A>>;

    unsafe fn finalize(self, static_buffers: Self::StaticInput) -> Self::Output {
        let grant_cap = create_capability!(capabilities::MemoryAllocationCapability);

        let virtual_alarm1 = static_buffers.0.initialize(VirtualMuxAlarm::new(self.alarm_mux));
        let alarm = static_buffers.1.initialize(AlarmDriver::new(
            virtual_alarm1,
            self.board_kernel.create_grant(&grant_cap),
        ));

        time::Alarm::set_client(virtual_alarm1, alarm);
        alarm
    }
}

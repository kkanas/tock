use core::fmt::Write;

use kernel;
use tock_cells::take_cell::TakeCell;

use crate::interrupt::{Interrupt, LowerHalf};
use crate::syscall::SysCall;
use crate::systick::SysTick;
use crate::Result;

use std::path::PathBuf;

/// An generic `Chip` implementation
pub struct HostChip<'a> {
    systick: SysTick,
    syscall: SysCall,
    irq_lower: TakeCell<'a, LowerHalf>,
}

impl<'a> HostChip<'a> {
    pub fn try_new(
        irq_lower: &'a mut LowerHalf,
        syscall_rx_path: PathBuf,
        syscall_tx_path: PathBuf,
    ) -> Result<HostChip<'a>> {
        Ok(HostChip {
            systick: SysTick::new(),
            syscall: SysCall::try_new(syscall_rx_path, syscall_tx_path)?,
            irq_lower: TakeCell::new(irq_lower),
        })
    }

    fn dispatch_interrupt(&self, _interrupt: Interrupt) {}

    fn irq_must_map<F, R>(&self, f: F) -> R
    where
        F: FnMut(&mut LowerHalf) -> R,
    {
        self.irq_lower
            .map_or_else(|| panic!("No irq lower half."), f)
    }
}

impl<'a> kernel::Chip for HostChip<'a> {
    type MPU = ();
    type UserspaceKernelBoundary = SysCall;
    type SysTick = SysTick;

    fn mpu(&self) -> &Self::MPU {
        &()
    }

    fn systick(&self) -> &Self::SysTick {
        &self.systick
    }

    fn userspace_kernel_boundary(&self) -> &SysCall {
        &self.syscall
    }

    fn service_pending_interrupts(&self) {
        self.irq_must_map(|irq_lower| {
            for interrupt in irq_lower {
                self.dispatch_interrupt(interrupt);
            }
        });
    }

    fn has_pending_interrupts(&self) -> bool {
        self.irq_must_map(|irq_lower| irq_lower.has_pending_interrupts())
    }

    fn sleep(&self) {
        let wait_untill = self.systick.get_systick_left();
        self.irq_must_map(|irq_lower| irq_lower.wait_for_interrupt(wait_untill));
    }

    unsafe fn atomic<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // TODO: What should the semantics be here? Is it okay to just call f()?
        f()
    }

    unsafe fn print_state(&self, writer: &mut dyn Write) {
        writer
            .write_fmt(format_args!("print_state() not implemented."))
            .unwrap();
    }
}

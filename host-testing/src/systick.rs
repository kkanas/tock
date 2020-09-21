use kernel;
use std::cell::Cell;
use std::time::{SystemTime, SystemTimeError};

pub struct SysTick {
    start_time: Cell<SystemTime>,
    set_duration_us: Cell<u32>,
    enabled: Cell<bool>,
}

impl SysTick {
    pub fn new() -> SysTick {
        SysTick {
            start_time: Cell::new(SystemTime::now()),
            set_duration_us: Cell::new(0),
            enabled: Cell::new(true),
        }
    }

    fn elapsed_us(&self) -> Result<u128, SystemTimeError> {
        let now = SystemTime::now();
        let elapsed_us = match now.duration_since(self.start_time.get()) {
            Ok(time) => time,
            Err(e) => return Err(e),
        };
        Ok(elapsed_us.as_micros())
    }
}

impl kernel::SysTick for SysTick {
    fn set_timer(&self, us: u32) {
        self.start_time.set(SystemTime::now());
        self.set_duration_us.set(us);
    }

    fn greater_than(&self, us: u32) -> bool {
        if !self.enabled.get() {
            return false;
        }
        let elapsed_us = match self.elapsed_us() {
            Ok(time) => time,
            Err(_) => return false,
        } ;
        let remaining_us = if self.set_duration_us.get() as u128 > elapsed_us {
            self.set_duration_us.get() as u128 - elapsed_us
        } else {
            0
        };
        return remaining_us >= us as u128;
    }

    fn overflowed(&self) -> bool {
        if !self.enabled.get() {
            return true;
        }

        let elapsed_us = match self.elapsed_us() {
            Ok(time) => time,
            Err(_) => return true,
        };
        return elapsed_us > self.set_duration_us.get() as u128;
    }

    fn reset(&self) {
        self.enabled.set(false);
        self.set_timer(0);
    }

    fn enable(&self, with_interrupt: bool) {
        self.enabled.set(true);
        if with_interrupt {
            panic!("Timer interrupts not implemented");
        }
    }
}

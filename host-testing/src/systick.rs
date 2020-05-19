use kernel;
use std::cell::Cell;
use std::time::{SystemTime, SystemTimeError};

pub struct SysTick {
    start_time: Cell<SystemTime>,
    offset: Cell<u32>,
    enabled: Cell<bool>,
}

impl SysTick {
    pub fn new() -> SysTick {
        SysTick {
            start_time: Cell::new(SystemTime::now()),
            offset: Cell::new(0),
            enabled: Cell::new(true),
        }
    }

    fn delta_us(&self) -> Result<u128, SystemTimeError> {
        let now = SystemTime::now();
        let delta = match now.duration_since(self.start_time.get()) {
            Ok(time) => time,
            Err(e) => return Err(e),
        };
        Ok(delta.as_micros())
    }
}

impl kernel::SysTick for SysTick {
    fn set_timer(&self, us: u32) {
        self.start_time.set(SystemTime::now());
        self.offset.set(us);
    }

    fn greater_than(&self, us: u32) -> bool {
        let delta = match self.delta_us() {
            Ok(time) => time,
            Err(_) => return false,
        } as u32;
        self.enabled.get() && delta > us
    }

    fn overflowed(&self) -> bool {
        let delta = match self.delta_us() {
            Ok(time) => time,
            Err(_) => return true,
        };
        delta > u32::max_value() as u128
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

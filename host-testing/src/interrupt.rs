use std::cmp::{Eq, Ord, Ordering, PartialOrd};
use std::collections::BinaryHeap;
use std::mem::{size_of, transmute};
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

use crate::Result;

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, Eq)]
pub struct Interrupt {
    source: u32,
}

pub struct UpperHalf {
    sender: Sender<Interrupt>,
    source: UnixDatagram,
}

pub struct LowerHalf {
    receiver: Receiver<Interrupt>,
    pending: BinaryHeap<Interrupt>,
}

impl PartialEq for Interrupt {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl PartialOrd for Interrupt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Interrupt {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source.cmp(&other.source)
    }
}

/// Creates a new interrupt "upper half" and "lower half" pair for servicing
/// external interrupts.
pub fn new_interrupt_channel(external_source: &Path) -> Result<(UpperHalf, LowerHalf)> {
    let socket = UnixDatagram::bind(external_source)?;
    let (sender, receiver): (Sender<Interrupt>, Receiver<Interrupt>) = channel();
    Ok((UpperHalf::new(sender, socket), LowerHalf::new(receiver)))
}

impl UpperHalf {
    fn new(sender: Sender<Interrupt>, source: UnixDatagram) -> UpperHalf {
        UpperHalf {
            sender: sender,
            source: source,
        }
    }

    pub fn spin(&self) -> Result<()> {
        let mut buf: [u8; size_of::<Interrupt>()] = [0; size_of::<Interrupt>()];
        loop {
            self.source.recv(&mut buf)?;
            let interrupt = unsafe { transmute::<[u8; size_of::<Interrupt>()], Interrupt>(buf) };
            if let Err(e) = self.sender.send(interrupt) {
                kernel::debug!("Interrupt upper half exiting: {}", e);
                return Err(crate::EmulationError::ChannelError);
            }
        }
    }
}

impl LowerHalf {
    fn new(receiver: Receiver<Interrupt>) -> LowerHalf {
        LowerHalf {
            receiver: receiver,
            pending: BinaryHeap::new(),
        }
    }

    fn receive_interrupts(&mut self, block: bool) {
        let interrupt = if block {
            match self.receiver.recv() {
                Ok(inter) => Ok(inter),
                Err(e) => Err(TryRecvError::from(e)),
            }
        } else {
            self.receiver.try_recv()
        };

        match interrupt {
            Ok(interrupt) => self.pending.push(interrupt),
            Err(TryRecvError::Empty) => {
                if !block {
                    return;
                }
                kernel::debug!("Failed to receive interrupt: {}", TryRecvError::Empty);
            }
            Err(e) => {
                kernel::debug!("Failed to receive interrupt: {}", e);
            }
        };
    }

    pub fn wait_for_interrupt(&mut self) -> Interrupt {
        kernel::debug!("Sleeping...");
        self.receive_interrupts(true);
        match self.pending.pop() {
            Some(interrupt) => interrupt,
            None => panic!("Received empty interrupt."),
        }
    }

    pub fn has_pending_interrupts(&mut self) -> bool {
        self.receive_interrupts(false);
        self.pending.peek().is_some()
    }
}

impl Iterator for &mut LowerHalf {
    type Item = Interrupt;

    fn next(&mut self) -> Option<Interrupt> {
        self.receive_interrupts(false);
        self.pending.pop()
    }
}

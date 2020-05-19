use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned};

use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};

use crate::EmulationError;
use crate::Result;

pub struct SyscallTransport {
    rx_path: PathBuf,
    tx_path: PathBuf,
    rx: UnixDatagram,
    tx: UnixDatagram,
}

impl SyscallTransport {
    pub fn open(rx_path: PathBuf, tx_path: PathBuf) -> Result<SyscallTransport> {
        let tx = UnixDatagram::unbound()?;
        let rx = UnixDatagram::bind(&rx_path)?;

        Ok(SyscallTransport {
            rx_path,
            tx_path,
            rx,
            tx,
        })
    }

    pub fn tx_path(&self) -> &Path {
        self.tx_path.as_path()
    }

    pub fn rx_path(&self) -> &Path {
        self.rx_path.as_path()
    }

    pub fn send<T: AsBytes>(&self, id: usize, data: &T) -> Result<()> {
        let bytes = data.as_bytes();
        self.send_bytes(id, bytes)?;
        Ok(())
    }

    pub fn recv<'b, T: FromBytes + Unaligned>(&self, buf: &'b mut [u8]) -> Result<&'b T> {
        self.rx.recv(buf)?;
        match LayoutVerified::<_, T>::new_unaligned(buf) {
            Some(msg) => Ok(msg.into_ref()),
            None => {
                return Err(EmulationError::Custom(format!(
                    "Failed to deserialize {}.",
                    std::any::type_name::<T>()
                )))
            }
        }
    }

    pub fn send_bytes(&self, _id: usize, bytes: &[u8]) -> Result<()> {
        let sent = self.tx.send(bytes)?;
        if sent != bytes.len() {
            Err(EmulationError::PartialMessage(bytes.len(), sent))
        } else {
            Ok(())
        }
    }

    pub fn recv_bytes(&self, buf: &mut [u8]) -> Result<()> {
        let receved = self.rx.recv(buf)?;
        if receved != buf.len() {
            Err(EmulationError::PartialMessage(buf.len(), receved))
        } else {
            Ok(())
        }
    }

    pub fn tx_connect_if_needed(&self) -> Result<()> {
        if let Err(_) = self.tx.peer_addr() {
            // We've either not yet connected or we've been disconnected.
            // Either way, attempt to connect.
            self.tx.connect(&self.tx_path)?;
        }

        Ok(())
    }
}

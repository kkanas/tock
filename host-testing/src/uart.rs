use std::io::{Read, Write};

use kernel;
use kernel::common::cells::OptionalCell;
use kernel::common::cells::TakeCell;
use kernel::hil::uart;
use kernel::hil::uart::{Configure, Receive, Transmit, Uart, UartData};
use kernel::ReturnCode;

pub struct UartIO<'a> {
    tx_stream: TakeCell<'a, dyn Write>,
    rx_stream: TakeCell<'a, dyn Read>,
    tx_client: OptionalCell<&'a dyn uart::TransmitClient>,
    rx_client: OptionalCell<&'a dyn uart::ReceiveClient>,
}

impl<'a> UartIO<'a> {
    pub fn new(rx_stream: &'a mut dyn Read, tx_stream: &'a mut dyn Write) -> UartIO<'a> {
        UartIO {
            tx_stream: TakeCell::new(tx_stream),
            rx_stream: TakeCell::new(rx_stream),
            tx_client: OptionalCell::empty(),
            rx_client: OptionalCell::empty(),
        }
    }
}

impl<'a> UartData<'a> for UartIO<'a> {}
impl<'a> Uart<'a> for UartIO<'a> {}

impl<'a> Configure for UartIO<'a> {
    fn configure(&self, _: uart::Parameters) -> kernel::ReturnCode {
        ReturnCode::SUCCESS
    }
}

impl<'a> Transmit<'a> for UartIO<'a> {
    fn set_transmit_client(&self, client: &'a dyn uart::TransmitClient) {
        self.tx_client.set(client);
    }

    fn transmit_buffer(
        &self,
        tx_data: &'static mut [u8],
        tx_len: usize,
    ) -> (ReturnCode, Option<&'static mut [u8]>) {
        if tx_len == 0 {
            return (ReturnCode::ESIZE, Some(tx_data));
        } else {
            self.tx_client.map(|client| {
                self.tx_stream.map(|stream| match stream.write(tx_data) {
                    Ok(_) => {
                        client.transmitted_buffer(tx_data, tx_len, ReturnCode::SUCCESS);
                        (ReturnCode::SUCCESS, None)
                    }
                    Err(_) => (ReturnCode::FAIL, Some(tx_data)),
                });
            });
            (ReturnCode::FAIL, None)
        }
    }

    fn transmit_abort(&self) -> ReturnCode {
        ReturnCode::FAIL
    }

    fn transmit_word(&self, _: u32) -> ReturnCode {
        ReturnCode::FAIL
    }
}

impl<'a> Receive<'a> for UartIO<'a> {
    fn set_receive_client(&self, client: &'a dyn uart::ReceiveClient) {
        self.rx_client.set(client);
    }

    fn receive_buffer(
        &self,
        rx_data: &'static mut [u8],
        rx_len: usize,
    ) -> (ReturnCode, Option<&'static mut [u8]>) {
        if rx_len == 0 {
            (ReturnCode::ESIZE, Some(rx_data))
        } else {
            self.rx_client.map(|client| {
                self.rx_stream.map(|stream| match stream.read(rx_data) {
                    Ok(_) => {
                        client.received_buffer(
                            rx_data,
                            rx_len,
                            ReturnCode::SUCCESS,
                            uart::Error::None,
                        );
                        (ReturnCode::SUCCESS, None)
                    }
                    Err(_) => (ReturnCode::FAIL, Some(rx_data)),
                });
            });
            (ReturnCode::FAIL, None)
        }
    }

    fn receive_word(&self) -> ReturnCode {
        ReturnCode::FAIL
    }

    fn receive_abort(&self) -> ReturnCode {
        ReturnCode::FAIL
    }
}

use std::io::Error;

use saito_core::common::handle_io::HandleIo;

pub struct WasmIoHandler {}

impl HandleIo for WasmIoHandler {
    fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error> {
        todo!()
    }

    fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
}

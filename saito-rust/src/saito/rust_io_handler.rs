use std::io::Error;

use saito_core::common::handle_io::HandleIo;

pub struct RustIOHandler {}

impl HandleIo for RustIOHandler {
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

    fn get_timestamp(&self) -> u64 {
        todo!()
    }
}

use std::io::Error;

use saito_core::common::handle_io::HandleIo;

pub struct RustIOHandler {}

impl HandleIo for RustIOHandler {
    fn send_message(&self, message_name: String, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
}

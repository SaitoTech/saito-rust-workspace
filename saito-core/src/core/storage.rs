use std::io::Error;

use crate::common::handle_io::HandleIo;

pub struct Storage {
    io_handler: dyn HandleIo,
}

impl Storage {
    pub fn save_data(&mut self, key: String, value: Vec<u8>) -> Result<(), Error> {
        self.io_handler.write_value(key, value)
    }

    pub fn load_value(&self, key: String) -> Result<Vec<u8>, Error> {
        self.io_handler.read_value(key)
    }
}

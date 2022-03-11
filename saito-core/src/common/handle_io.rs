use std::io::Error;

pub trait HandleIo {
    fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error>;
    fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error>;
    fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;
}

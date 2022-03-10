use std::io::Error;

pub enum SaitoEvent {
    NetworkRead,
    FileRead,
}

pub struct Saito {}

impl Saito {
    pub fn send_message(&self) {}
    pub fn process_message(&self, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }
    pub fn process_disk_read(&self, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }
}

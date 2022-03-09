use std::io::Error;

pub enum SaitoEvent {
    NetworkRead,
    FileRead,
}

pub struct Saito {}

impl Saito {
    fn send_message(&self) {}
    fn process_message(&self, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }
    fn process_disk_read(&self, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }
}

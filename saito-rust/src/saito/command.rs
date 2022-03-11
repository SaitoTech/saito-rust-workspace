use std::io::Error;

pub enum Command {
    NetworkMessage(u64, Vec<u8>),
    DataSaveRequest(String, Vec<u8>),
    // can do without this.
    DataSaveResponse(String, Error),
    DataReadRequest(String),
    DataReadResponse(String, Vec<u8>, Error),
}

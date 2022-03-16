use std::io::Error;

use crate::common::defs::Hash32;

pub enum Command {
    NetworkMessage(u64, Vec<u8>),
    DataSaveRequest(String, Vec<u8>),
    // can do without this.
    DataSaveResponse(String, Error),
    DataReadRequest(String),
    DataReadResponse(String, Vec<u8>, Error),
    ConnectToPeer(String),
    PeerConnected(u64, String),
    PeerDisconnected(String),
}

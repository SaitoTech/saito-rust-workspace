use std::sync::{Arc, RwLock};

use crate::common::handle_io::HandleIo;
use crate::core::blockchain::Blockchain;
use crate::core::context::Context;
use crate::core::mempool::Mempool;

pub struct Saito<T: HandleIo> {
    pub io_handler: T,
    pub context: Context,
}

impl<T: HandleIo> Saito<T> {
    pub fn process_message_buffer(&mut self, peer_index: u64, buffer: Vec<u8>) {
        todo!()
    }
    pub fn on_timer(&mut self) -> Option<()> {
        todo!()
    }
}

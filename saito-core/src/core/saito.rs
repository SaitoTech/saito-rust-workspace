use std::sync::{Arc, RwLock};

use crate::common::handle_io::HandleIo;
use crate::core::blockchain::Blockchain;
use crate::core::context::Context;
use crate::core::mempool::Mempool;

pub struct Saito<T: HandleIo> {
    pub io_handler: T,
    pub context: Context,
}

impl<T: HandleIo> Saito<T> {}

use std::sync::{Arc, RwLock};

use crate::core::blockchain::Blockchain;
use crate::core::mempool::Mempool;

pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
}

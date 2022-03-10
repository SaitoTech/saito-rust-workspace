use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use crate::common::defs::{PrivateKey, PublicKey};
use crate::core::data::block::Block;
use crate::core::data::transaction::Transaction;
use crate::core::wallet::Wallet;

pub struct Mempool {
    blocks_queue: VecDeque<Block>,
    pub transactions: Vec<Transaction>,
    // vector so we just copy it over
    routing_work_in_mempool: u64,
    wallet_lock: Arc<RwLock<Wallet>>,
    currently_bundling_block: bool,
    mempool_publickey: PublicKey,
    mempool_privatekey: PrivateKey,
}

impl Mempool {
    pub fn add_block(&mut self, block: Block) {
        todo!()
    }
}

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
    wallet: Arc<RwLock<Wallet>>,
    currently_bundling_block: bool,
    public_key: PublicKey,
    private_key: PrivateKey,
}

impl Mempool {
    pub fn new(wallet: Arc<RwLock<Wallet>>) -> Mempool {
        Mempool {
            blocks_queue: Default::default(),
            transactions: vec![],
            routing_work_in_mempool: 0,
            wallet,
            currently_bundling_block: false,
            public_key: [0; 33],
            private_key: [0; 32],
        }
    }
    pub fn add_block(&mut self, block: Block) {
        todo!()
    }
}

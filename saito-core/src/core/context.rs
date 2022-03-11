use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::core::blockchain::Blockchain;
use crate::core::mempool::Mempool;
use crate::core::peer::Peer;
use crate::core::wallet::Wallet;

pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub peers: HashMap<u64, Peer>,
}

impl Context {
    pub fn new() -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet,
            peers: Default::default(),
        }
    }
}

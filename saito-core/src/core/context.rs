use std::collections::HashMap;
use std::io::Error;
use std::sync::{Arc, RwLock};

use crate::common::run_task::RunTask;
use crate::core::blockchain::Blockchain;
use crate::core::mempool::Mempool;
use crate::core::miner::Miner;
use crate::core::peer::Peer;
use crate::core::wallet::Wallet;

pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub peers: HashMap<u64, Peer>,
    pub miner: Arc<RwLock<Miner>>,
}

impl Context {
    pub fn new() -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet,
            peers: Default::default(),
            miner: Arc::new(RwLock::new(Miner::new())),
        }
    }
    pub fn init(&self, task_runner: &dyn RunTask) -> Result<(), Error> {
        self.miner.write().unwrap().init(task_runner)?;
        self.mempool.write().unwrap().init(task_runner)?;
        self.blockchain.write().unwrap().init()?;

        Ok(())
    }
}

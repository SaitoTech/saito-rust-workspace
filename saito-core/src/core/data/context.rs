use std::collections::HashMap;
use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::command::SaitoEvent;
use crate::common::run_task::RunTask;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::mempool::Mempool;
use crate::core::data::miner::Miner;
use crate::core::data::peer::Peer;
use crate::core::data::wallet::Wallet;

pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub peers: HashMap<u64, Peer>,
    pub miner: Arc<RwLock<Miner>>,
}

impl Context {
    pub fn new(global_sender: tokio::sync::broadcast::Sender<SaitoEvent>) -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(
                wallet.clone(),
                global_sender.clone(),
            ))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet,
            peers: Default::default(),
            miner: Arc::new(RwLock::new(Miner::new())),
        }
    }
    pub async fn init(&self, task_runner: &dyn RunTask) -> Result<(), Error> {
        self.miner.write().await.init(task_runner)?;
        self.mempool.write().await.init(task_runner)?;
        self.blockchain.write().await.init()?;

        Ok(())
    }
}

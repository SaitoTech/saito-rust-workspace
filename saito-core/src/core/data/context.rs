use std::collections::HashMap;
use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::command::GlobalEvent;
use crate::common::run_task::RunTask;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::mempool::Mempool;
use crate::core::data::miner::Miner;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::wallet::Wallet;

pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub miner: Arc<RwLock<Miner>>,
    pub configuration: Arc<RwLock<Configuration>>,
}

impl Context {
    pub fn new(
        configs: Arc<RwLock<Configuration>>,
        global_sender: tokio::sync::broadcast::Sender<GlobalEvent>,
    ) -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(
                wallet.clone(),
                // global_sender.clone(),
            ))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet: wallet.clone(),
            peers: Arc::new(RwLock::new(PeerCollection::new())),
            miner: Arc::new(RwLock::new(Miner::new(wallet.clone()))),
            configuration: configs,
        }
    }
    pub async fn init(&self, task_runner: &dyn RunTask) -> Result<(), Error> {
        // self.miner.write().await.init(task_runner)?;
        // self.mempool.write().await.init(task_runner)?;
        // self.blockchain.write().await.init()?;

        Ok(())
    }
}

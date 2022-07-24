use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::run_task::RunTask;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::mempool::Mempool;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::wallet::Wallet;

#[derive(Clone)]
pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub configuration: Arc<RwLock<Configuration>>,
    pub peers: Arc<RwLock<PeerCollection>>,
}

impl Context {
    pub fn new(configuration: Arc<RwLock<Configuration>>) -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let blockchain = Arc::new(RwLock::new(Blockchain::new(wallet.clone())));
        let peers = Arc::new(RwLock::new(PeerCollection::new(
            configuration.clone(),
            blockchain.clone(),
            wallet.clone(),
        )));

        Context {
            blockchain,
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet: wallet.clone(),
            configuration,
            peers,
        }
    }
    pub async fn init(&self, _task_runner: &dyn RunTask) -> Result<(), Error> {
        // self.miner.write().await.init(task_runner)?;
        // self.mempool.write().await.init(task_runner)?;
        // self.blockchain.write().await.init()?;

        Ok(())
    }
}

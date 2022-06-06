use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::run_task::RunTask;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::mempool::Mempool;
use crate::core::data::wallet::Wallet;

#[derive(Clone)]
pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub configuration: Arc<RwLock<Configuration>>,
}

impl Context {
    pub fn new(configs: Arc<RwLock<Configuration>>) -> Context {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(
                wallet.clone(),
                // global_sender.clone(),
            ))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet: wallet.clone(),
            configuration: configs,
        }
    }
    pub async fn init(&self, _task_runner: &dyn RunTask) -> Result<(), Error> {
        // self.miner.write().await.init(task_runner)?;
        // self.mempool.write().await.init(task_runner)?;
        // self.blockchain.write().await.init()?;

        Ok(())
    }
}

use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::wallet::Wallet;
use crate::core::process::run_task::RunTask;
use crate::core::util::configuration::Configuration;

#[derive(Clone)]
pub struct Context {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub configuration: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl Context {
    pub fn new(
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
        wallet: Arc<RwLock<Wallet>>,
    ) -> Context {
        Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet,
            configuration: configs,
        }
    }
    pub async fn init(&self, _task_runner: &dyn RunTask) -> Result<(), Error> {
        Ok(())
    }
}

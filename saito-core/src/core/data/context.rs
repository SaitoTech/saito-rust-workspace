use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::defs::{SaitoPrivateKey, SaitoPublicKey};
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

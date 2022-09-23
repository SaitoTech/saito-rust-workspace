use crate::saito::config_handler::SpammerConfigs;
use crate::saito::transaction_generator::TransactionGenerator;

use crate::{IoEvent};


use saito_core::common::command::NetworkEvent;

use saito_core::core::data::blockchain::Blockchain;

use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::msg::message::Message;

use saito_core::core::data::wallet::Wallet;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use tracing::info;

pub struct Spammer {
    blockchain: Arc<RwLock<Blockchain>>,
    mempool: Arc<RwLock<Mempool>>,
    wallet: Arc<RwLock<Wallet>>,
    sender_to_network: Sender<IoEvent>,
    configs: Arc<RwLock<Box<SpammerConfigs>>>,
    bootstrap_done: bool,
    sent_tx_count: u64,
    tx_generator: TransactionGenerator,
}

impl Spammer {
    pub async fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        mempool: Arc<RwLock<Mempool>>,
        wallet: Arc<RwLock<Wallet>>,
        sender_to_network: Sender<IoEvent>,
        configs: Arc<RwLock<Box<SpammerConfigs>>>,
    ) -> Spammer {
        Spammer {
            blockchain,
            mempool,
            wallet: wallet.clone(),
            sender_to_network,
            configs: configs.clone(),
            bootstrap_done: false,
            sent_tx_count: 0,
            tx_generator: TransactionGenerator::create(wallet.clone(), configs.clone()).await,
        }
    }
    pub async fn execute(&mut self) -> bool {
        let txs = self.tx_generator.on_new_block().await;
        if txs.is_none() {
            return false;
        }
        let txs = txs.unwrap();
        for tx in txs {
            self.sent_tx_count += 1;
            self.sender_to_network
                .send(IoEvent {
                    event_processor_id: 0,
                    event_id: 0,
                    event: NetworkEvent::OutgoingNetworkMessageForAll {
                        buffer: Message::Transaction(tx).serialize(),
                        exceptions: vec![],
                    },
                })
                .await;
        }
        {
            let wallet = self.wallet.read().await;
            info!(
                "sent tx count : {:?} total slip count : {:?} current balance : {:?}",
                self.sent_tx_count,
                wallet.slips.len(),
                wallet.get_available_balance()
            );
        }
        return true;
    }
}

pub async fn run_spammer(
    blockchain: Arc<RwLock<Blockchain>>,
    mempool: Arc<RwLock<Mempool>>,
    wallet: Arc<RwLock<Wallet>>,
    sender_to_network: Sender<IoEvent>,
    configs: Arc<RwLock<Box<SpammerConfigs>>>,
) {
    info!("starting the spammer");
    let mut spammer = Spammer::new(blockchain, mempool, wallet, sender_to_network, configs).await;
    loop {
        let mut work_done = false;
        work_done = spammer.execute().await;
        if !work_done {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
}

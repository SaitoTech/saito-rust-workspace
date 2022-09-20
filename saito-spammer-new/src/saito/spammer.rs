use crate::saito::config_handler::SpammerConfigs;
use crate::saito::transaction_generator::TransactionGenerator;
use crate::{IoEvent, TimeKeeper};
use saito_core::common::command::NetworkEvent;
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::slip::Slip;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::routing_event_processor::RoutingEvent;
use std::cmp::min;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace};

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
            )
        }
        // if !self.bootstrap_done {
        //     // self.bootstrap().await;
        //     return true;
        // }
        return true;
    }
    async fn bootstrap(&mut self) {
        info!("bootstrapping...");
        let time_keeper = TimeKeeper {};
        let public_key;
        let private_key;
        let slips;
        {
            let wallet = self.wallet.read().await;
            slips = wallet.slips.clone();
            public_key = wallet.public_key.clone();
            private_key = wallet.private_key.clone();
        }
        for slip in slips.iter() {
            if slip.amount > 1 && !slip.spent {
                self.bootstrap_done = false;
                let slip_count = min(100, slip.amount as u8);
                let mut slip_value = 1;
                if slip.amount > 100 {
                    slip_value = slip.amount / 100;
                }
                debug!(
                    "breaking slip with value : {:?} into {:?} slips with value : {:?}",
                    slip.amount, slip_count, slip_value
                );

                let mut transaction = Transaction::new();

                let mut input_slip = Slip::new();
                input_slip.uuid = slip.uuid;
                input_slip.public_key = public_key.clone();
                input_slip.amount = slip.amount;
                input_slip.utxoset_key = slip.utxokey;
                transaction.add_input(input_slip);

                for i in 0..slip_count {
                    let mut slip = Slip::new();
                    slip.amount = slip_value;
                    slip.public_key = public_key.clone();
                    slip.slip_index = i;
                    transaction.add_output(slip);
                }
                transaction.timestamp = time_keeper.get_timestamp();
                transaction.generate(public_key);
                transaction.sign(private_key);
                transaction.add_hop(self.wallet.clone(), public_key).await;

                debug!(
                    "sending tx {:?} with {:?} outputs",
                    hex::encode(transaction.hash_for_signature.unwrap()),
                    transaction.outputs.len()
                );
                self.sent_tx_count += 1;
                self.sender_to_network
                    .send(IoEvent {
                        event_processor_id: 0,
                        event_id: 0,
                        event: NetworkEvent::OutgoingNetworkMessageForAll {
                            buffer: Message::Transaction(transaction).serialize(),
                            exceptions: vec![],
                        },
                    })
                    .await;
            }
        }
        {
            let wallet = self.wallet.read().await;
            info!(
                "sent tx count : {:?} total slip count : {:?} current balance : {:?}",
                self.sent_tx_count,
                wallet.slips.len(),
                wallet.get_available_balance()
            )
        }
    }
    async fn send_txs(&mut self) {
        let public_key;
        let wallet = self.wallet.read().await;
        public_key = wallet.public_key.clone();
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
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

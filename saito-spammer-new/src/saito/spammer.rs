use crate::saito::config_handler::SpammerConfigs;
use crate::IoEvent;
use saito_core::common::command::NetworkEvent;
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
use tracing::{info, trace};

pub struct Spammer {
    blockchain: Arc<RwLock<Blockchain>>,
    mempool: Arc<RwLock<Mempool>>,
    wallet: Arc<RwLock<Wallet>>,
    sender_to_network: Sender<IoEvent>,
    configs: Arc<RwLock<Box<SpammerConfigs>>>,
    bootstrap_done: bool,
    sent_tx_count: u64,
}

impl Spammer {
    pub fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        mempool: Arc<RwLock<Mempool>>,
        wallet: Arc<RwLock<Wallet>>,
        sender_to_network: Sender<IoEvent>,
        configs: Arc<RwLock<Box<SpammerConfigs>>>,
    ) -> Spammer {
        Spammer {
            blockchain,
            mempool,
            wallet,
            sender_to_network,
            configs,
            bootstrap_done: false,
            sent_tx_count: 0,
        }
    }
    pub async fn execute(&mut self) -> bool {
        if !self.bootstrap_done {
            self.bootstrap();
            return true;
        }
        return false;
    }
    async fn bootstrap(&mut self) {
        let slips_clone;
        let public_key;
        {
            let wallet = self.wallet.write().await;
            public_key = wallet.public_key.clone();
            slips_clone = wallet.slips.clone();
        }
        self.bootstrap_done = true;
        for slip in slips_clone.iter() {
            if slip.amount > 1 {
                self.bootstrap_done = false;
                let slip_count = min(100, slip.amount as u8);
                let mut slip_value = 1;
                if slip.amount > 100 {
                    slip_value = slip.amount / 100;
                }
                trace!(
                    "breaking slip with value : {:?} into {:?} slips with value : {:?}",
                    slip.amount,
                    slip_count,
                    slip_value
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

                transaction.generate(public_key);
                {
                    let wallet = self.wallet.read().await;
                    transaction.sign(wallet.private_key);
                }
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
        info!("sent tx count : {:?}", self.sent_tx_count);
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
    let mut spammer = Spammer::new(blockchain, mempool, wallet, sender_to_network, configs);
    loop {
        let mut work_done = false;
        work_done = spammer.execute().await;
        if !work_done {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

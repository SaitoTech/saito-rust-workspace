use log::{debug, trace};
use saito_core::common::command::NetworkEvent;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::msg::block_request::BlockchainRequest;
use saito_core::core::data::msg::handshake::HandshakeChallenge;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::IoEvent;

pub struct SpamGenerator {
    wallet: Arc<RwLock<Wallet>>,
    blockchain: Arc<RwLock<Blockchain>>,
}

impl SpamGenerator {
    pub fn new(wallet: Arc<RwLock<Wallet>>, blockchain: Arc<RwLock<Blockchain>>) -> Self {
        SpamGenerator { wallet, blockchain }
    }

    pub async fn run(
        generator: SpamGenerator,
        timer_in_milli: u64,
        count: u32,
        bytes_per_tx: u32,
        sender: Sender<IoEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(timer_in_milli));
            loop {
                interval.tick().await;

                let transactions = generator.generate_tx(count, bytes_per_tx).await;

                for transaction in transactions {
                    let message = Message::Transaction(transaction);
                    let buffer = message.serialize();
                    let peer_exceptions = Default::default();
                    let event = IoEvent::new(NetworkEvent::OutgoingNetworkMessageForAll {
                        buffer,
                        exceptions: peer_exceptions,
                    });

                    sender.send(event).await;
                }
            }
        })
    }

    async fn generate_tx(&self, count: u32, bytes_per_tx: u32) -> Vec<Transaction> {
        trace!("generating mock transactions");

        let mut transactions: Vec<Transaction> = Default::default();

        let public_key;
        let private_key;
        //let latest_block_id;
        {
            trace!("waiting for the wallet read lock");
            let wallet = self.wallet.read().await;
            trace!("acquired the wallet read lock");
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        {
            trace!("waiting for the blockchain read lock");
            let blockchain = self.blockchain.read().await;
            trace!("acquired the blockchain read lock");

            if blockchain.blockring.is_empty() {
                return transactions;
            }

            // if latest_block_id == 0 {
            //     let mut vip_transaction =
            //         Transaction::create_vip_transaction(public_key, 50_000_000);
            //     vip_transaction.sign(private_key);
            //     transactions.push(vip_transaction);
            // }
        }

        for _i in 0..count {
            let mut transaction =
                Transaction::create(self.wallet.clone(), public_key, 100, 100).await;
            transaction.message = (0..bytes_per_tx)
                .into_iter()
                .map(|_| rand::random::<u8>())
                .collect();
            transaction.generate(public_key);
            transaction.sign(private_key);

            transaction.add_hop(self.wallet.clone(), public_key).await;
            // transaction
            //     .add_hop(self.wallet.clone(), public_key)
            //     .await;

            transactions.push(transaction);
        }

        trace!("generated transaction count: {:?}", transactions.len());

        return transactions;
    }
}

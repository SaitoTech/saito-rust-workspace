use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::mempool::Mempool;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::core::miner_controller::MinerEvent;

pub enum MempoolEvent {}

pub struct MempoolController {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub sender_global: tokio::sync::broadcast::Sender<GlobalEvent>,
    pub io_handler: Box<dyn HandleIo + Send>,
}

impl MempoolController {
    // TODO : rename function
    pub async fn send_blocks_to_blockchain(&mut self) {
        let mut mempool = self.mempool.write().await;
        let mut blockchain = self.blockchain.write().await;
        while let Some(block) = mempool.blocks_queue.pop_front() {
            mempool.delete_transactions(&block.get_transactions());
            blockchain
                .add_block(block, self.sender_global.clone())
                .await;
        }
    }
    async fn generate_tx(&self) {
        debug!("generating mock transactions");

        let mempool_lock_clone = self.mempool.clone();
        let wallet_lock_clone = self.wallet.clone();
        let blockchain_lock_clone = self.blockchain.clone();

        let txs_to_generate = 10;
        let bytes_per_tx = 1024;
        let publickey;
        let privatekey;
        let latest_block_id;

        {
            let wallet = wallet_lock_clone.read().await;
            publickey = wallet.get_publickey();
            privatekey = wallet.get_privatekey();
        }

        {
            let blockchain = blockchain_lock_clone.read().await;
            latest_block_id = blockchain.get_latest_block_id();
        }

        {
            if latest_block_id == 0 {
                let mut vip_transaction = Transaction::generate_vip_transaction(
                    wallet_lock_clone.clone(),
                    publickey,
                    100_000_000,
                    10,
                )
                .await;
                vip_transaction.sign(privatekey);
                let mut mempool = mempool_lock_clone.write().await;
                mempool.add_transaction(vip_transaction).await;
            }
        }

        loop {
            for _i in 0..txs_to_generate {
                let mut transaction = Transaction::generate_transaction(
                    wallet_lock_clone.clone(),
                    publickey,
                    5000,
                    5000,
                )
                .await;
                transaction.set_message(
                    (0..bytes_per_tx)
                        .into_iter()
                        .map(|_| rand::random::<u8>())
                        .collect(),
                );
                transaction.sign(privatekey);
                // before validation!
                transaction.generate_metadata(publickey);

                transaction
                    .add_hop_to_path(wallet_lock_clone.clone(), publickey)
                    .await;
                transaction
                    .add_hop_to_path(wallet_lock_clone.clone(), publickey)
                    .await;
                {
                    let mut mempool = mempool_lock_clone.write().await;
                    mempool
                        .add_transaction_if_validates(transaction, blockchain_lock_clone.clone())
                        .await;
                }
            }
            info!("TXS TO GENERATE: {:?}", txs_to_generate);
        }
    }
}

#[async_trait]
impl ProcessEvent<MempoolEvent> for MempoolController {
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        trace!("processing new interface event");
        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        trace!("processing timer event : {:?}", duration.as_micros());

        let mut can_bundle = false;
        let timestamp = self.io_handler.get_timestamp();
        // TODO : create mock transactions here
        self.generate_tx();

        {
            let mempool = self.mempool.read().await;
            can_bundle = mempool
                .can_bundle_block(self.blockchain.clone(), timestamp)
                .await;
        }
        if can_bundle {
            let mempool = self.mempool.clone();
            let mut mempool = mempool.write().await;
            let result = mempool
                .bundle_block(self.blockchain.clone(), timestamp)
                .await;
            mempool.add_block(result);
            self.send_blocks_to_blockchain();
        }
        None
    }

    async fn process_event(&mut self, event: MempoolEvent) -> Option<()> {
        None
    }
}

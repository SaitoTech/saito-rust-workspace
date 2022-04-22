use std::borrow::BorrowMut;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::mempool::Mempool;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::core::miner_controller::MinerEvent;

pub enum MempoolEvent {
    NewGoldenTicket { golden_ticket: GoldenTicket },
}

pub struct MempoolController {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    // pub sender_global: tokio::sync::broadcast::Sender<GlobalEvent>,
    pub block_producing_timer: u128,
    pub tx_producing_timer: u128,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
}

impl MempoolController {
    // TODO : rename function
    pub async fn send_blocks_to_blockchain(&mut self, mempool: &mut Mempool) {
        debug!("send blocks to blockchain");
        // TODO : refactor to use channel instead of directly calling the blockchain methods
        while let Some(block) = mempool.blocks_queue.pop_front() {
            mempool.delete_transactions(&block.get_transactions());
            self.sender_to_blockchain
                .send(BlockchainEvent::NewBlockBundled(block))
                .await
                .unwrap();
        }
        debug!("blocks sent to blockchain");
    }
    async fn generate_tx(
        mempool: Arc<RwLock<Mempool>>,
        wallet: Arc<RwLock<Wallet>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        debug!("generating mock transactions");

        let mempool_lock_clone = mempool.clone();
        let wallet_lock_clone = wallet.clone();
        let blockchain_lock_clone = blockchain.clone();

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

        for _i in 0..txs_to_generate {
            let mut transaction =
                Transaction::generate_transaction(wallet_lock_clone.clone(), publickey, 5000, 5000)
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
                let blockchain = blockchain_lock_clone.read().await;
                let mut mempool = mempool_lock_clone.write().await;
                mempool
                    .add_transaction_if_validates(transaction, &blockchain)
                    .await;
            }
        }
        trace!("generated transaction count: {:?}", txs_to_generate);
    }
}

#[async_trait]
impl ProcessEvent<MempoolEvent> for MempoolController {
    async fn process_global_event(&mut self, _event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, _event: InterfaceEvent) -> Option<()> {
        debug!("processing new interface event");

        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());
        let mut work_done = false;

        let timestamp = self.time_keeper.get_timestamp();

        let duration_value = duration.as_micros();

        // generate test transactions
        self.tx_producing_timer = self.tx_producing_timer + duration_value;
        if self.tx_producing_timer >= 1_000_000 {
            // TODO : Remove this transaction generation once testing is done
            MempoolController::generate_tx(
                self.mempool.clone(),
                self.wallet.clone(),
                self.blockchain.clone(),
            )
            .await;

            self.tx_producing_timer = 0;
            work_done = true;
        }

        // generate blocks
        let mut can_bundle = false;
        self.block_producing_timer = self.block_producing_timer + duration_value;
        // TODO : make timers configurable
        if self.block_producing_timer >= 1_000_000 {
            debug!("producing block...");
            let mempool = self.mempool.read().await;
            can_bundle = mempool
                .can_bundle_block(self.blockchain.clone(), timestamp)
                .await;
            self.block_producing_timer = 0;
            work_done = true;
        }

        if can_bundle {
            let mempool = self.mempool.clone();
            let mut mempool = mempool.write().await;
            let result = mempool
                .bundle_block(self.blockchain.clone(), timestamp)
                .await;
            mempool.add_block(result);
            self.send_blocks_to_blockchain(mempool.borrow_mut()).await;
            work_done = true;
        }

        if work_done {
            return Some(());
        }
        None
    }

    async fn process_event(&mut self, event: MempoolEvent) -> Option<()> {
        match event {
            MempoolEvent::NewGoldenTicket { golden_ticket } => {
                let mut mempool = self.mempool.write().await;
                mempool.add_golden_ticket(golden_ticket).await;
            }
        }
        None
    }

    async fn on_init(&mut self) {}
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::core::data::block::Block;

    use super::*;

    #[test]
    fn mempool_new_test() {
        let wallet = Wallet::new();
        let mempool = Mempool::new(Arc::new(RwLock::new(wallet)));
        assert_eq!(mempool.blocks_queue, VecDeque::new());
    }

    #[test]
    fn mempool_add_block_test() {
        let wallet = Wallet::new();
        let mut mempool = Mempool::new(Arc::new(RwLock::new(wallet)));
        let block = Block::new();
        mempool.add_block(block.clone());
        assert_eq!(Some(block), mempool.blocks_queue.pop_front())
    }
}

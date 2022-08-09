use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::SaitoPublicKey;
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::mempool::Mempool;
use crate::core::data::network::Network;

use crate::core::data::storage::Storage;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::core::mining_event_processor::MiningEvent;
use crate::core::routing_event_processor::RoutingEvent;

#[derive(Debug)]
pub enum ConsensusEvent {
    NewGoldenTicket { golden_ticket: GoldenTicket },
    BlockFetched { peer_index: u64, buffer: Vec<u8> },
    NewTransaction { transaction: Transaction },
}

/// Manages blockchain and the mempool
pub struct ConsensusEventProcessor {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_router: Sender<RoutingEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub block_producing_timer: u128,
    pub tx_producing_timer: u128,
    pub create_test_tx: bool,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub network: Network,
    pub storage: Storage,
}

impl ConsensusEventProcessor {
    /// Test method to generate test transactions
    ///
    /// # Arguments
    ///
    /// * `mempool`:
    /// * `wallet`:
    /// * `blockchain`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn generate_tx(
        mempool: Arc<RwLock<Mempool>>,
        wallet: Arc<RwLock<Wallet>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        trace!("generating mock transactions");

        let mempool_lock_clone = mempool.clone();
        let wallet_lock_clone = wallet.clone();
        let blockchain_lock_clone = blockchain.clone();

        let txs_to_generate = 10;
        let bytes_per_tx = 1024;
        let public_key;
        let private_key;
        let latest_block_id;

        {
            trace!("waiting for the wallet read lock");
            let wallet = wallet_lock_clone.read().await;
            trace!("acquired the wallet read lock");
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        trace!("waiting for the mempool write lock");
        let mut mempool = mempool_lock_clone.write().await;
        trace!("acquired the mempool write lock");
        trace!("waiting for the blockchain read lock");
        let blockchain = blockchain_lock_clone.read().await;
        trace!("acquired the blockchain read lock");

        latest_block_id = blockchain.get_latest_block_id();

        let spammer_public_key: SaitoPublicKey =
            hex::decode("03145c7e7644ab277482ba8801a515b8f1b62bcd7e4834a33258f438cd7e223849")
                .unwrap()
                .try_into()
                .unwrap();

        {
            if latest_block_id == 0 {
                let mut vip_transaction =
                    Transaction::create_vip_transaction(public_key, 50_000_000);
                vip_transaction.sign(private_key);

                mempool.add_transaction(vip_transaction).await;

                let mut vip_transaction =
                    Transaction::create_vip_transaction(spammer_public_key, 50_000_000);
                vip_transaction.sign(private_key);

                mempool.add_transaction(vip_transaction).await;
            }
        }

        for _i in 0..txs_to_generate {
            let mut transaction =
                Transaction::create(wallet_lock_clone.clone(), public_key, 5000, 5000).await;
            transaction.message = (0..bytes_per_tx)
                .into_iter()
                .map(|_| rand::random::<u8>())
                .collect();
            transaction.generate(public_key);
            transaction.sign(private_key);

            transaction
                .add_hop(wallet_lock_clone.clone(), public_key)
                .await;
            // transaction
            //     .add_hop(wallet_lock_clone.clone(), public_key)
            //     .await;
            {
                mempool
                    .add_transaction_if_validates(transaction, &blockchain)
                    .await;
            }
        }
        trace!("generated transaction count: {:?}", txs_to_generate);
    }
}

#[async_trait]
impl ProcessEvent<ConsensusEvent> for ConsensusEventProcessor {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        trace!("processing new interface event");

        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());
        let mut work_done = false;

        let timestamp = self.time_keeper.get_timestamp();

        let duration_value = duration.as_micros();

        // generate test transactions
        if self.create_test_tx {
            self.tx_producing_timer = self.tx_producing_timer + duration_value;
            if self.tx_producing_timer >= 1_000_000 {
                // TODO : Remove this transaction generation once testing is done
                ConsensusEventProcessor::generate_tx(
                    self.mempool.clone(),
                    self.wallet.clone(),
                    self.blockchain.clone(),
                )
                .await;

                self.tx_producing_timer = 0;
                work_done = true;
            }
        }

        // generate blocks
        let mut can_bundle = false;
        self.block_producing_timer = self.block_producing_timer + duration_value;
        // TODO : make timers configurable
        if self.block_producing_timer >= 1_000_000 {
            trace!("waiting for the mempool read lock");
            let mempool = self.mempool.read().await;
            trace!("acquired the mempool read lock");
            can_bundle = mempool
                .can_bundle_block(self.blockchain.clone(), timestamp)
                .await;
            self.block_producing_timer = 0;
            work_done = true;
        }

        if can_bundle {
            let mempool = self.mempool.clone();
            trace!("waiting for the mempool write lock");
            let mut mempool = mempool.write().await;
            trace!("acquired the mempool write lock");
            trace!("waiting for the blockchain write lock");
            let mut blockchain = self.blockchain.write().await;
            trace!("acquired the blockchain write lock");
            let result = mempool
                .bundle_block(blockchain.deref_mut(), timestamp)
                .await;
            mempool.add_block(result);

            debug!("adding blocks to blockchain");

            while let Some(block) = mempool.blocks_queue.pop_front() {
                trace!(
                    "deleting transactions from block : {:?}",
                    hex::encode(block.hash)
                );
                mempool.delete_transactions(&block.transactions);
                blockchain
                    .add_block(
                        block,
                        &mut self.network,
                        &mut self.storage,
                        self.sender_to_miner.clone(),
                    )
                    .await;
            }
            debug!("blocks added to blockchain");

            work_done = true;
        }

        if work_done {
            return Some(());
        }
        None
    }

    async fn process_event(&mut self, event: ConsensusEvent) -> Option<()> {
        match event {
            ConsensusEvent::NewGoldenTicket { golden_ticket } => {
                debug!(
                    "received new golden ticket : {:?}",
                    hex::encode(golden_ticket.target)
                );
                trace!("waiting for the mempool write lock");
                let mut mempool = self.mempool.write().await;
                trace!("acquired the mempool write lock");
                mempool.add_golden_ticket(golden_ticket).await;
            }
            ConsensusEvent::BlockFetched {
                peer_index: _,
                buffer,
            } => {
                let mut blockchain = self.blockchain.write().await;
                let block = Block::deserialize_from_net(&buffer);
                blockchain
                    .add_block(
                        block,
                        &mut self.network,
                        &mut self.storage,
                        self.sender_to_miner.clone(),
                    )
                    .await;
            }
            ConsensusEvent::NewTransaction { mut transaction } => {
                transaction.generate_hash_for_signature();
                debug!("tx received with sig: {:?}", transaction.signature);
                trace!("waiting for the mempool write lock");
                let mut mempool = self.mempool.write().await;
                trace!(
                    "acquired the mempool write lock, adding new transaction : {:?}",
                    hex::encode(transaction.hash_for_signature.unwrap())
                );
                mempool.add_transaction(transaction).await;
            }
        }
        None
    }

    async fn on_init(&mut self) {
        debug!("on_init");
        self.storage
            .load_blocks_from_disk(
                self.blockchain.clone(),
                &self.network,
                self.sender_to_miner.clone(),
            )
            .await;
    }
}

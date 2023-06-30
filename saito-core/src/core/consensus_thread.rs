use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::{
    push_lock, SaitoPrivateKey, SaitoPublicKey, StatVariable, Timestamp, LOCK_ORDER_BLOCKCHAIN,
    LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET, STAT_BIN_COUNT,
};
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::crypto::hash;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::mempool::Mempool;
use crate::core::data::network::Network;
use crate::core::data::slip;
use crate::core::data::storage::Storage;
use crate::core::data::transaction::{Transaction, TransactionType};
use crate::core::data::wallet::Wallet;
use crate::core::mining_thread::MiningEvent;
use crate::core::routing_thread::RoutingEvent;
use crate::{lock_for_read, lock_for_write};

use super::data::wallet;

pub const BLOCK_PRODUCING_TIMER: u64 = Duration::from_millis(100).as_millis() as u64;
pub const SPAM_TX_PRODUCING_TIMER: u64 = Duration::from_millis(1_000).as_millis() as u64;

#[derive(Debug)]
pub enum ConsensusEvent {
    NewGoldenTicket { golden_ticket: GoldenTicket },
    BlockFetched { peer_index: u64, block: Block },
    NewTransaction { transaction: Transaction },
    NewTransactions { transactions: Vec<Transaction> },
}

pub struct ConsensusStats {
    pub blocks_fetched: StatVariable,
    pub blocks_created: StatVariable,
    pub received_tx: StatVariable,
    pub received_gts: StatVariable,
}

impl ConsensusStats {
    pub fn new(sender: Sender<String>) -> Self {
        ConsensusStats {
            blocks_fetched: StatVariable::new(
                "consensus::blocks_fetched".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
            blocks_created: StatVariable::new(
                "consensus::blocks_created".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
            received_tx: StatVariable::new(
                "consensus::received_tx".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
            received_gts: StatVariable::new(
                "consensus::received_gts".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
        }
    }
}

/// Manages blockchain and the mempool
pub struct ConsensusThread {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub generate_genesis_block: bool,
    pub sender_to_router: Sender<RoutingEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub block_producing_timer: u64,
    pub tx_producing_timer: u64,
    pub create_test_tx: bool,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub network: Network,
    pub storage: Storage,
    pub stats: ConsensusStats,
    pub txs_for_mempool: Vec<Transaction>,
    pub stat_sender: Sender<String>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl ConsensusThread {
    async fn generate_spammer_init_tx(
        mempool: Arc<RwLock<Mempool>>,
        wallet: Arc<RwLock<Wallet>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        info!("generating spammer init transaction");

        let private_key;

        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
            private_key = wallet.private_key;
        }

        let (blockchain, _blockchain_) = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);
        let (mut mempool, _mempool_) = lock_for_write!(mempool, LOCK_ORDER_MEMPOOL);

        let spammer_public_key: SaitoPublicKey =
            hex::decode("03145c7e7644ab277482ba8801a515b8f1b62bcd7e4834a33258f438cd7e223849")
                .unwrap()
                .try_into()
                .unwrap();

        {
            // let mut vip_transaction = Transaction::create_vip_transaction(public_key, 50_000_000);
            // vip_transaction.sign(&private_key);
            //
            // mempool
            //     .add_transaction_if_validates(vip_transaction, &blockchain)
            //     .await;

            let mut vip_transaction =
                Transaction::create_vip_transaction(spammer_public_key, 100_000_000);
            vip_transaction.sign(&private_key);

            mempool
                .add_transaction_if_validates(vip_transaction, &blockchain)
                .await;
            info!(
                "added spammer init tx for : {:?}",
                hex::encode(spammer_public_key)
            );
        }
    }
    async fn generate_issuance_tx(
        &self,
        _mempool: Arc<RwLock<Mempool>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        info!("generating issuance init transaction");

        let slips = self.storage.get_token_supply_slips_from_disk().await;
        let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
        let mut txs: Vec<Transaction> = vec![];
        for slip in slips {
            debug!("{:?} slip public key", hex::encode(slip.public_key));
            let mut tx = Transaction::create_issuance_transaction(slip.public_key, slip.amount);
            tx.sign(&wallet.private_key);
            txs.push(tx);
        }

        let (blockchain, _blockchain_) = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);
        let (mut mempool, _mempool_) = lock_for_write!(_mempool, LOCK_ORDER_MEMPOOL);

        // debug!("{:?} transaction from slips", txs);
        for tx in txs {
            mempool
                .add_transaction_if_validates(tx.clone(), &blockchain)
                .await;
            info!("added issuance init tx for : {:?}", tx.signature);
        }

        debug!("{:?} mempool transacts", mempool.transactions);
    }
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
        info!("generating mock transactions");
        let txs_to_generate = 10;
        let bytes_per_tx = 1024;
        let public_key;
        let private_key;

        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let (blockchain, _blockchain_) = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);
        let (mut mempool, _mempool_) = lock_for_write!(mempool, LOCK_ORDER_MEMPOOL);

        let latest_block_id = blockchain.get_latest_block_id();

        let spammer_public_key: SaitoPublicKey =
            hex::decode("03145c7e7644ab277482ba8801a515b8f1b62bcd7e4834a33258f438cd7e223849")
                .unwrap()
                .try_into()
                .unwrap();

        {
            if latest_block_id == 0 {
                let mut vip_transaction =
                    Transaction::create_vip_transaction(public_key, 50_000_000);
                vip_transaction.sign(&private_key);

                mempool
                    .add_transaction_if_validates(vip_transaction, &blockchain)
                    .await;

                let mut vip_transaction =
                    Transaction::create_vip_transaction(spammer_public_key, 50_000_000);
                vip_transaction.sign(&private_key);

                mempool
                    .add_transaction_if_validates(vip_transaction, &blockchain)
                    .await;
            }
        }

        let (mut wallet, _wallet_) = lock_for_write!(wallet, LOCK_ORDER_WALLET);

        for _i in 0..txs_to_generate {
            let mut transaction;
            transaction = Transaction::create(&mut wallet, public_key, 5000, 5000, false).unwrap();
            // TODO : generate a message buffer which can be converted back into JSON
            transaction.data = (0..bytes_per_tx).map(|_| rand::random::<u8>()).collect();
            transaction.generate(&public_key, 0, 0);
            transaction.sign(&private_key);

            transaction.add_hop(&private_key, &public_key, &public_key);
            {
                mempool
                    .add_transaction_if_validates(transaction, &blockchain)
                    .await;
            }
        }
        info!("generated transaction count: {:?}", txs_to_generate);
    }
}

#[async_trait]
impl ProcessEvent<ConsensusEvent> for ConsensusThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        unreachable!();
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());
        let mut work_done = false;
        let timestamp = self.time_keeper.get_timestamp_in_ms();
        let duration_value = duration.as_millis() as u64;

        if self.generate_genesis_block {
            Self::generate_issuance_tx(self, self.mempool.clone(), self.blockchain.clone()).await;

            {
                let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
                let (mut blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
                if blockchain.blocks.is_empty() && blockchain.genesis_block_id == 0 {
                    let (mut mempool, _mempool_) =
                    lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);
                                        
                    let block: Block = (mempool
                        .bundle_block(&mut blockchain, timestamp, None, configs.deref(), true)
                        .await).unwrap();

                    println!(" block ider {:?}", &block.transactions);

                    let res = blockchain
                        .add_block(
                            block,
                            &self.network,
                            &mut self.storage,
                            self.sender_to_miner.clone(),
                            &mut mempool,
                            configs.deref(),
                        )
                        .await;
                }

                // println!("{} addblock result", result)
                self.generate_genesis_block = false;
                return Some(());
            }
        }

        // generate test transactions
        if self.create_test_tx {
            self.tx_producing_timer += duration_value;
            if self.tx_producing_timer >= SPAM_TX_PRODUCING_TIMER {
                // TODO : Remove this transaction generation once testing is done
                ConsensusThread::generate_tx(
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
        self.block_producing_timer += duration_value;
        if self.block_producing_timer >= BLOCK_PRODUCING_TIMER {
            let (configs, _configs_) = lock_for_read!(self.network.configs, LOCK_ORDER_CONFIGS);

            let (mut blockchain, _blockchain_) =
                lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
            let (mut mempool, _mempool_) = lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);

            if !self.txs_for_mempool.is_empty() {
                for tx in self.txs_for_mempool.iter() {
                    if let TransactionType::GoldenTicket = tx.transaction_type {
                        unreachable!("golden tickets shouldn't be here");
                    } else {
                        mempool.add_transaction(tx.clone()).await;
                    }
                }
            }

            self.block_producing_timer = 0;

            // trace!(
            //     "mempool size before bundling : {:?}",
            //     mempool.transactions.len()
            // );
            let mut gt_result = None;
            let mut gt_propagated = false;
            {
                let result: Option<&(Transaction, bool)> = mempool
                    .golden_tickets
                    .get(&blockchain.get_latest_block_hash());
                if let Some((tx, propagated)) = result {
                    gt_result = Some(tx.clone());
                    gt_propagated = *propagated;
                }
            }

            let mut block = None;
            if !configs.is_browser() && !configs.is_spv_mode() {
                block = mempool
                    .bundle_block(
                        blockchain.deref_mut(),
                        timestamp,
                        gt_result.clone(),
                        configs.deref(),
                        false
                    )
                    .await;
            }
            if block.is_some() {
                let block = block.unwrap();
                info!(
                    "adding bundled block : {:?} with id : {:?} to mempool",
                    hex::encode(block.hash),
                    block.id
                );
                trace!(
                    "mempool size after bundling : {:?}",
                    mempool.transactions.len()
                );

                mempool.add_block(block);
                self.txs_for_mempool.clear();
                // dropping the lock here since blockchain needs the write lock to add blocks
                drop(mempool);
                self.stats.blocks_created.increment();
                let updated = blockchain
                    .add_blocks_from_mempool(
                        self.mempool.clone(),
                        &self.network,
                        &mut self.storage,
                        self.sender_to_miner.clone(),
                        configs.deref(),
                    )
                    .await;

                if updated {
                    self.sender_to_router
                        .send(RoutingEvent::BlockchainUpdated)
                        .await
                        .unwrap();
                }

                debug!("blocks added to blockchain");

                work_done = true;
            } else {
                // route messages to peers
                for tx in self.txs_for_mempool.drain(..) {
                    self.network.propagate_transaction(&tx).await;
                }
                // route golden tickets to peers
                if gt_result.is_some() && !gt_propagated {
                    self.network
                        .propagate_transaction(gt_result.as_ref().unwrap())
                        .await;
                    debug!(
                        "propagating gt : {:?} to peers",
                        hex::encode(hash(&gt_result.unwrap().serialize_for_net()))
                    );
                    let (_, propagated) = mempool
                        .golden_tickets
                        .get_mut(&blockchain.get_latest_block_hash())
                        .unwrap();
                    *propagated = true;
                }
            }
        }

        if work_done {
            return Some(());
        }
        None
    }

    async fn process_event(&mut self, event: ConsensusEvent) -> Option<()> {
        return match event {
            ConsensusEvent::NewGoldenTicket { golden_ticket } => {
                debug!(
                    "received new golden ticket : {:?}",
                    hex::encode(golden_ticket.target)
                );

                let (mut mempool, _mempool_) = lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);
                let public_key;
                let private_key;
                {
                    let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);

                    public_key = wallet.public_key;
                    private_key = wallet.private_key;
                }
                let transaction = Wallet::create_golden_ticket_transaction(
                    golden_ticket,
                    &public_key,
                    &private_key,
                )
                .await;
                self.stats.received_gts.increment();
                mempool.add_golden_ticket(transaction).await;
                Some(())
            }
            ConsensusEvent::BlockFetched { block, .. } => {
                let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
                let (mut blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);

                {
                    debug!("block : {:?} fetched from peer", hex::encode(block.hash));

                    if blockchain.blocks.contains_key(&block.hash) {
                        debug!(
                            "fetched block : {:?} already in blockchain",
                            hex::encode(block.hash)
                        );
                        return Some(());
                    }
                    debug!("adding fetched block to mempool");
                    let (mut mempool, _mempool_) =
                        lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);
                    mempool.add_block(block);
                }
                self.stats.blocks_fetched.increment();
                let updated = blockchain
                    .add_blocks_from_mempool(
                        self.mempool.clone(),
                        &self.network,
                        &mut self.storage,
                        self.sender_to_miner.clone(),
                        configs.deref(),
                    )
                    .await;

                if updated {
                    self.sender_to_router
                        .send(RoutingEvent::BlockchainUpdated)
                        .await
                        .unwrap();
                }

                Some(())
            }
            ConsensusEvent::NewTransaction { transaction } => {
                self.stats.received_tx.increment();

                trace!(
                    "tx received with sig: {:?} hash : {:?}",
                    hex::encode(transaction.signature),
                    hex::encode(hash(&transaction.serialize_for_net()))
                );
                if let TransactionType::GoldenTicket = transaction.transaction_type {
                    let (mut mempool, _mempool_) =
                        lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);

                    self.stats.received_gts.increment();
                    mempool.add_golden_ticket(transaction).await;
                } else {
                    self.txs_for_mempool.push(transaction);
                }

                Some(())
            }
            ConsensusEvent::NewTransactions { mut transactions } => {
                self.stats
                    .received_tx
                    .increment_by(transactions.len() as u64);

                self.txs_for_mempool.reserve(transactions.len());
                for transaction in transactions.drain(..) {
                    if let TransactionType::GoldenTicket = transaction.transaction_type {
                        let (mut mempool, _mempool_) =
                            lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);

                        self.stats.received_gts.increment();
                        mempool.add_golden_ticket(transaction).await;
                    } else {
                        self.txs_for_mempool.push(transaction);
                    }
                }
                Some(())
            }
        };
    }

    async fn on_init(&mut self) {
        debug!("on_init");
        self.storage
            .load_blocks_from_disk(self.mempool.clone())
            .await;

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);

        {
            let (mempool, _mempool_) = lock_for_read!(self.mempool, LOCK_ORDER_MEMPOOL);
            if configs.get_peer_configs().is_empty() && mempool.blocks_queue.is_empty() {
                self.generate_genesis_block = true;
            }
        }

        blockchain
            .add_blocks_from_mempool(
                self.mempool.clone(),
                &self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
                configs.deref(),
            )
            .await;
    }

    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        self.stats
            .blocks_fetched
            .calculate_stats(current_time)
            .await;
        self.stats
            .blocks_created
            .calculate_stats(current_time)
            .await;
        self.stats.received_tx.calculate_stats(current_time).await;
        self.stats.received_gts.calculate_stats(current_time).await;

        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);

            let stat = format!(
                "{} - total_slips : {:?}, unspent_slips : {:?}, current_balance : {:?}",
                format!("{:width$}", "wallet::state", width = 40),
                wallet.slips.len(),
                wallet.get_unspent_slip_count(),
                wallet.get_available_balance()
            );
            self.stat_sender.send(stat).await.unwrap();
        }
        {
            let (blockchain, _blockchain_) = lock_for_read!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
            let stat = format!(
                "{} - utxo_size : {:?}, block_count : {:?}, longest_chain_len : {:?}",
                format!("{:width$}", "blockchain::state", width = 40),
                blockchain.utxoset.len(),
                blockchain.blocks.len(),
                blockchain.get_latest_block_id()
            );
            self.stat_sender.send(stat).await.unwrap();
        }
        {
            let (mempool, _mempool_) = lock_for_read!(self.mempool, LOCK_ORDER_MEMPOOL);

            let stat = format!(
                "{} - blocks_queue : {:?}, transactions : {:?}",
                format!("{:width$}", "mempool:state", width = 40),
                mempool.blocks_queue.len(),
                mempool.transactions.len(),
            );
            self.stat_sender.send(stat).await.unwrap();
        }
    }
}

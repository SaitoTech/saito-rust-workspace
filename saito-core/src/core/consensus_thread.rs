use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::core::consensus::block::{Block, BlockType};
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::golden_ticket::GoldenTicket;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::transaction::{Transaction, TransactionType};
use crate::core::consensus::wallet::Wallet;
use crate::core::defs::{
    PrintForLog, SaitoHash, StatVariable, Timestamp, CHANNEL_SAFE_BUFFER, STAT_BIN_COUNT,
};
use crate::core::io::network::Network;
use crate::core::io::network_event::NetworkEvent;
use crate::core::io::storage::Storage;
use crate::core::mining_thread::MiningEvent;
use crate::core::process::keep_time::Timer;
use crate::core::process::process_event::ProcessEvent;
use crate::core::routing_thread::RoutingEvent;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::hash;

pub const BLOCK_PRODUCING_TIMER: u64 = Duration::from_millis(1000).as_millis() as u64;

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
    pub mempool_lock: Arc<RwLock<Mempool>>,
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub generate_genesis_block: bool,
    pub sender_to_router: Sender<RoutingEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub block_producing_timer: u64,
    pub timer: Timer,
    pub network: Network,
    pub storage: Storage,
    pub stats: ConsensusStats,
    pub txs_for_mempool: Vec<Transaction>,
    pub stat_sender: Sender<String>,
    pub config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub produce_blocks_by_timer: bool,
}

impl ConsensusThread {
    async fn generate_issuance_tx(
        &self,
        mempool_lock: Arc<RwLock<Mempool>>,
        blockchain_lock: Arc<RwLock<Blockchain>>,
    ) {
        info!("generating issuance init transaction");

        let slips = self.storage.get_token_supply_slips_from_disk().await;
        let private_key;
        let public_key;
        {
            let wallet = self.wallet_lock.read().await;
            private_key = wallet.private_key;
            public_key = wallet.public_key;
        }
        let mut txs: Vec<Transaction> = vec![];
        for slip in slips {
            debug!("{:?} slip public key", slip.public_key.to_base58());
            let mut tx = Transaction::create_issuance_transaction(slip.public_key, slip.amount);
            tx.generate(&public_key, 0, 0);
            tx.sign(&private_key);
            txs.push(tx);
        }

        let blockchain = blockchain_lock.read().await;
        let mut mempool = mempool_lock.write().await;

        for tx in txs {
            mempool
                .add_transaction_if_validates(tx.clone(), &blockchain)
                .await;
            info!("added issuance init tx for : {:?}", tx.signature.to_hex());
        }
    }
    pub async fn produce_block(
        &mut self,
        timestamp: Timestamp,
        gt_result: Option<&Transaction>,
        mempool: &mut Mempool,
        blockchain: &Blockchain,
        configs: &(dyn Configuration + Send + Sync),
    ) -> Option<Block> {
        // trace!("locking blockchain 3");

        self.block_producing_timer = 0;

        let block = mempool
            .bundle_block(
                blockchain,
                timestamp,
                gt_result.cloned(),
                configs,
                &self.storage,
            )
            .await;
        if let Some(block) = block {
            trace!(
                "mempool size after creating block : {:?}",
                mempool.transactions.len()
            );

            self.txs_for_mempool.clear();

            return Some(block);
        }
        None
    }
    pub async fn bundle_block(
        &mut self,
        timestamp: Timestamp,
        produce_without_limits: bool,
    ) -> bool {
        let config_lock = self.config_lock.clone();
        let configs = config_lock.read().await;

        // trace!("locking blockchain 3");
        let blockchain_lock = self.blockchain_lock.clone();
        let mempool_lock = self.mempool_lock.clone();
        let mut blockchain = blockchain_lock.write().await;
        let mut mempool = mempool_lock.write().await;

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
        if (produce_without_limits || (!configs.is_browser() && !configs.is_spv_mode()))
            && !blockchain.blocks.is_empty()
        {
            block = self
                .produce_block(
                    timestamp,
                    gt_result.as_ref(),
                    &mut mempool,
                    &blockchain,
                    configs.deref(),
                )
                .await;
        } else {
            // debug!("skipped bundling block. : produce_without_limits = {:?}, is_browser : {:?} block_count : {:?}",
            //     produce_without_limits,
            //     configs.is_browser() || configs.is_spv_mode(),
            //     blockchain.blocks.len());
        }
        if let Some(block) = block {
            debug!(
                "adding bundled block : {:?} with id : {:?} to mempool",
                block.hash.to_hex(),
                block.id
            );
            trace!(
                "mempool size after bundling : {:?}",
                mempool.transactions.len()
            );

            mempool.add_block(block);
            // dropping the lock here since blockchain needs the write lock to add blocks
            drop(mempool);
            self.stats.blocks_created.increment();
            let _updated = blockchain
                .add_blocks_from_mempool(
                    self.mempool_lock.clone(),
                    Some(&self.network),
                    &mut self.storage,
                    Some(self.sender_to_miner.clone()),
                    Some(self.sender_to_router.clone()),
                    configs.deref(),
                )
                .await;

            debug!("blocks added to blockchain");
            return true;
        } else {
            // route messages to peers
            if !self.txs_for_mempool.is_empty() {
                trace!(
                    "since a block was not produced, propagating {:?} txs to peers",
                    self.txs_for_mempool.len()
                );
                for tx in self.txs_for_mempool.drain(..) {
                    self.network.propagate_transaction(&tx).await;
                }
            }
            // route golden tickets to peers
            if gt_result.is_some() && !gt_propagated {
                self.network
                    .propagate_transaction(gt_result.as_ref().unwrap())
                    .await;
                debug!(
                    "propagating gt : {:?} to peers",
                    hash(&gt_result.unwrap().serialize_for_net()).to_hex()
                );
                let (_, propagated) = mempool
                    .golden_tickets
                    .get_mut(&blockchain.get_latest_block_hash())
                    .unwrap();
                *propagated = true;
            }
            return true;
        }
        // trace!("releasing blockchain 3");
        false
    }

    async fn produce_genesis_block(&mut self, timestamp: Timestamp) {
        Self::generate_issuance_tx(
            self,
            self.mempool_lock.clone(),
            self.blockchain_lock.clone(),
        )
        .await;

        let configs = self.config_lock.read().await;
        let mut blockchain = self.blockchain_lock.write().await;
        if blockchain.blocks.is_empty() && blockchain.genesis_block_id == 0 {
            let mut mempool = self.mempool_lock.write().await;

            let block = mempool
                .bundle_genesis_block(&mut blockchain, timestamp, configs.deref(), &self.storage)
                .await;

            let _res = blockchain
                .add_block(block, &mut self.storage, &mut mempool, configs.deref())
                .await;
        }

        self.generate_genesis_block = false;
    }

    pub async fn add_gt_to_mempool(&mut self, golden_ticket: GoldenTicket) {
        debug!(
            "adding received new golden ticket : {:?} to mempool",
            golden_ticket.target.to_hex()
        );
        let mut mempool = self.mempool_lock.write().await;
        let public_key;
        let private_key;
        {
            let wallet = self.wallet_lock.read().await;

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }
        let transaction =
            Wallet::create_golden_ticket_transaction(golden_ticket, &public_key, &private_key)
                .await;
        self.stats.received_gts.increment();
        mempool.add_golden_ticket(transaction).await;
    }
}

#[async_trait]
impl ProcessEvent<ConsensusEvent> for ConsensusThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        unreachable!();
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        let mut work_done = false;
        let timestamp = self.timer.get_timestamp_in_ms();
        let duration_value = duration.as_millis() as u64;

        if self.generate_genesis_block {
            self.produce_genesis_block(timestamp).await;
            return Some(());
        }

        // generate blocks
        self.block_producing_timer += duration_value;
        if self.produce_blocks_by_timer && self.block_producing_timer >= BLOCK_PRODUCING_TIMER {
            self.bundle_block(timestamp, false).await;

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
                self.add_gt_to_mempool(golden_ticket).await;
                Some(())
            }
            ConsensusEvent::BlockFetched { block, .. } => {
                let configs = self.config_lock.read().await;
                // trace!("locking blockchain 4");
                let mut blockchain = self.blockchain_lock.write().await;

                {
                    debug!("block : {:?} fetched from peer", block.hash.to_hex());

                    if blockchain.blocks.contains_key(&block.hash) {
                        debug!(
                            "fetched block : {:?} already in blockchain",
                            block.hash.to_hex()
                        );
                        return Some(());
                    }
                    debug!(
                        "adding fetched block : {:?}-{:?} to mempool",
                        block.id,
                        block.hash.to_hex()
                    );
                    let mut mempool = self.mempool_lock.write().await;
                    mempool.add_block(block);
                }
                self.stats.blocks_fetched.increment();
                blockchain
                    .add_blocks_from_mempool(
                        self.mempool_lock.clone(),
                        Some(&self.network),
                        &mut self.storage,
                        Some(self.sender_to_miner.clone()),
                        Some(self.sender_to_router.clone()),
                        configs.deref(),
                    )
                    .await;
                // trace!("releasing blockchain 4");

                Some(())
            }
            ConsensusEvent::NewTransaction { transaction } => {
                self.stats.received_tx.increment();

                if let TransactionType::GoldenTicket = transaction.transaction_type {
                    let mut mempool = self.mempool_lock.write().await;

                    self.stats.received_gts.increment();
                    trace!("adding golden ticket to mempool");
                    mempool.add_golden_ticket(transaction).await;
                } else {
                    trace!("adding transaction to mempool");
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
                        let mut mempool = self.mempool_lock.write().await;

                        self.stats.received_gts.increment();
                        mempool.add_golden_ticket(transaction).await;
                    } else {
                        self.txs_for_mempool.push(transaction);
                    }
                }
                Some(())
            }
        }
    }

    async fn on_init(&mut self) {
        debug!("on_init");

        {
            let configs = self.config_lock.read().await;
            info!(
                "genesis_period : {:?}",
                configs.get_consensus_config().unwrap().genesis_period
            );
            let mut blockchain = self.blockchain_lock.write().await;
            let blockchain_configs = configs.get_blockchain_configs();
            if let Some(blockchain_configs) = blockchain_configs {
                info!(
                    "loading blockchain state from configs : {:?}",
                    blockchain_configs
                );
                blockchain.last_block_hash =
                    SaitoHash::from_hex(blockchain_configs.last_block_hash.as_str())
                        .unwrap_or([0; 32]);
                blockchain.last_block_id = blockchain_configs.last_block_id;
                blockchain.last_timestamp = blockchain_configs.last_timestamp;
                blockchain.genesis_block_id = blockchain_configs.genesis_block_id;
                blockchain.genesis_timestamp = blockchain_configs.genesis_timestamp;
                blockchain.lowest_acceptable_timestamp =
                    blockchain_configs.lowest_acceptable_timestamp;
                blockchain.lowest_acceptable_block_hash =
                    SaitoHash::from_hex(blockchain_configs.lowest_acceptable_block_hash.as_str())
                        .unwrap_or([0; 32]);
                blockchain.lowest_acceptable_block_id =
                    blockchain_configs.lowest_acceptable_block_id;
                blockchain.fork_id = Some(
                    SaitoHash::from_hex(blockchain_configs.fork_id.as_str()).unwrap_or([0; 32]),
                );
            } else {
                info!("blockchain state is not loaded");
            }
        }

        let configs = self.config_lock.read().await;
        let mut blockchain = self.blockchain_lock.write().await;
        if !configs.is_browser() {
            let mut list = self
                .storage
                .load_block_name_list()
                .await
                .expect("cannot load block file list");
            if configs.get_peer_configs().is_empty() && list.is_empty() {
                self.generate_genesis_block = true;
            }
            let start_time = self.timer.get_timestamp_in_ms();

            info!(
                "loading {:?} blocks from disk. Timestamp : {:?}",
                list.len(),
                StatVariable::format_timestamp(start_time)
            );
            while !list.is_empty() {
                let file_names: Vec<String> =
                    list.drain(..std::cmp::min(1000, list.len())).collect();
                self.storage
                    .load_blocks_from_disk(file_names.as_slice(), self.mempool_lock.clone())
                    .await;

                blockchain
                    .add_blocks_from_mempool(
                        self.mempool_lock.clone(),
                        None,
                        &mut self.storage,
                        None,
                        None,
                        configs.deref(),
                    )
                    .await;

                info!(
                    "{:?} blocks remaining to be loaded. Timestamp : {:?}",
                    list.len(),
                    StatVariable::format_timestamp(self.timer.get_timestamp_in_ms())
                );
            }
            info!(
                "{:?} total blocks in blockchain. Timestamp : {:?}, elapsed_time : {:?}",
                blockchain.blocks.len(),
                StatVariable::format_timestamp(self.timer.get_timestamp_in_ms()),
                self.timer.get_timestamp_in_ms() - start_time
            );
        }

        debug!(
            "sending block id update as : {:?}",
            blockchain.last_block_id
        );
    }

    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        // println!("on_stat_interval : {:?}", current_time);

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
            let wallet = self.wallet_lock.read().await;

            let stat = format!(
                "{} - {} - total_slips : {:?}, unspent_slips : {:?}, current_balance : {:?}",
                StatVariable::format_timestamp(current_time),
                format!("{:width$}", "wallet::state", width = 40),
                wallet.slips.len(),
                wallet.get_unspent_slip_count(),
                wallet.get_available_balance()
            );
            self.stat_sender.send(stat).await.unwrap();
        }
        {
            let stat;
            {
                // trace!("locking blockchain 5");
                let blockchain = self.blockchain_lock.read().await;

                stat = format!(
                    "{} - {} - utxo_size : {:?}, block_count : {:?}, longest_chain_len : {:?} full_block_count : {:?} txs_in_blocks : {:?}",
                    StatVariable::format_timestamp(current_time),
                    format!("{:width$}", "blockchain::state", width = 40),
                    blockchain.utxoset.len(),
                    blockchain.blocks.len(),
                    blockchain.get_latest_block_id(),
                    blockchain.blocks.iter().filter(|(_hash, block)| { block.block_type == BlockType::Full }).count(),
                    blockchain.blocks.iter().map(|(_hash, block)| { block.transactions.len() }).sum::<usize>()
                );
            }
            // trace!("releasing blockchain 5");
            self.stat_sender.send(stat).await.unwrap();
        }
        {
            let stat;
            {
                let mempool = self.mempool_lock.read().await;

                stat = format!(
                    "{} - {} - blocks_queue : {:?}, transactions : {:?}",
                    StatVariable::format_timestamp(current_time),
                    format!("{:width$}", "mempool:state", width = 40),
                    mempool.blocks_queue.len(),
                    mempool.transactions.len(),
                );
            }

            self.stat_sender.send(stat).await.unwrap();
        }
        {
            let stat = format!(
                "{} - {} - capacity : {:?} / {:?}",
                StatVariable::format_timestamp(current_time),
                format!("{:width$}", "router::channel", width = 40),
                self.sender_to_router.capacity(),
                self.sender_to_router.max_capacity()
            );
            self.stat_sender.send(stat).await.unwrap();
        }
    }

    fn is_ready_to_process(&self) -> bool {
        self.sender_to_miner.capacity() > CHANNEL_SAFE_BUFFER
            && self.sender_to_router.capacity() > CHANNEL_SAFE_BUFFER
    }
}

#[cfg(test)]
mod tests {
    use crate::core::consensus::blockchain::DEFAULT_SOCIAL_STAKE_PERIOD;
    use crate::core::defs::{PrintForLog, NOLAN_PER_SAITO};
    use crate::core::util::crypto::generate_keys;
    use crate::core::util::test::node_tester::test::NodeTester;

    #[tokio::test]
    #[serial_test::serial]
    async fn total_supply_test() {
        // pretty_env_logger::init();
        NodeTester::delete_blocks().await.unwrap();
        let peer_public_key = generate_keys().0;
        let mut tester = NodeTester::default();
        let public_key = tester.get_public_key().await;
        // tester
        //     .set_staking_requirement(2_000_000 * NOLAN_PER_SAITO, 60)
        //     .await;
        let issuance = vec![
            // (public_key.to_base58(), 100 * 2_000_000 * NOLAN_PER_SAITO),
            (public_key.to_base58(), 100_000 * NOLAN_PER_SAITO),
            (
                "27UK2MuBTdeARhYp97XBnCovGkEquJjkrQntCgYoqj6GC".to_string(),
                50_000 * NOLAN_PER_SAITO,
            ),
        ];
        tester.set_issuance(issuance).await.unwrap();
        tester.init().await.unwrap();
        tester.wait_till_block_id(1).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let tx = tester
            .create_transaction(10_000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(2).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let tx = tester
            .create_transaction(10_000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(3).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let tx = tester
            .create_transaction(10_000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(4).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let tx = tester
            .create_transaction(10_000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(5).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
    }
    #[tokio::test]
    #[serial_test::serial]
    async fn total_supply_test_with_atr() {
        // pretty_env_logger::init();
        NodeTester::delete_blocks().await.unwrap();
        let peer_public_key = generate_keys().0;
        let mut tester = NodeTester::default();
        let public_key = tester.get_public_key().await;
        // tester
        //     .set_staking_requirement(2_000_000 * NOLAN_PER_SAITO, 60)
        //     .await;
        let issuance = vec![
            // (public_key.to_base58(), 100 * 2_000_000 * NOLAN_PER_SAITO),
            (public_key.to_base58(), 100_000 * NOLAN_PER_SAITO),
            (
                "27UK2MuBTdeARhYp97XBnCovGkEquJjkrQntCgYoqj6GC".to_string(),
                50_000 * NOLAN_PER_SAITO,
            ),
        ];
        tester.set_issuance(issuance).await.unwrap();
        tester.init().await.unwrap();
        tester.wait_till_block_id(1).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let genesis_period = tester
            .consensus_thread
            .config_lock
            .read()
            .await
            .get_consensus_config()
            .unwrap()
            .genesis_period;
        for i in 2..2 * genesis_period + 2 {
            let tx = tester.create_transaction(10, 10, public_key).await.unwrap();
            tester.add_transaction(tx).await;
            tester.wait_till_block_id(i).await.unwrap();
            tester
                .check_total_supply()
                .await
                .expect("total supply should not change");
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn total_supply_test_with_staking_for_slip_count() {
        // pretty_env_logger::init();
        NodeTester::delete_blocks().await.unwrap();
        let peer_public_key = generate_keys().0;
        let mut tester = NodeTester::default();
        let public_key = tester.get_public_key().await;
        tester
            .set_staking_requirement(2 * NOLAN_PER_SAITO, DEFAULT_SOCIAL_STAKE_PERIOD)
            .await;
        let issuance = vec![
            (
                public_key.to_base58(),
                DEFAULT_SOCIAL_STAKE_PERIOD * 2 * NOLAN_PER_SAITO,
            ),
            (public_key.to_base58(), 100 * NOLAN_PER_SAITO),
            (
                "27UK2MuBTdeARhYp97XBnCovGkEquJjkrQntCgYoqj6GC".to_string(),
                50 * NOLAN_PER_SAITO,
            ),
        ];
        tester.set_issuance(issuance).await.unwrap();
        tester.init().await.unwrap();
        tester.wait_till_block_id(1).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let genesis_period = tester
            .consensus_thread
            .config_lock
            .read()
            .await
            .get_consensus_config()
            .unwrap()
            .genesis_period;
        for i in 2..2 * genesis_period + 2 {
            let tx = tester.create_transaction(10, 10, public_key).await.unwrap();
            tester.add_transaction(tx).await;
            tester.wait_till_block_id(i).await.unwrap();
            tester
                .check_total_supply()
                .await
                .expect("total supply should not change");
            assert_eq!(
                tester
                    .consensus_thread
                    .wallet_lock
                    .read()
                    .await
                    .staking_slips
                    .len() as u64,
                std::cmp::min(DEFAULT_SOCIAL_STAKE_PERIOD, i - 1)
            );
            let available_balance = tester
                .consensus_thread
                .wallet_lock
                .read()
                .await
                .get_available_balance();
            assert!(
                available_balance
                    + std::cmp::min(DEFAULT_SOCIAL_STAKE_PERIOD, i - 1) * 2 * NOLAN_PER_SAITO
                    <= tester.initial_token_supply
            );
        }
    }

    #[ignore]
    #[tokio::test]
    #[serial_test::serial]
    async fn blockchain_state_over_atr() {
        pretty_env_logger::init();
        NodeTester::delete_blocks().await.unwrap();
        let peer_public_key = generate_keys().0;
        let mut tester = NodeTester::new(3);
        let public_key = tester.get_public_key().await;
        // tester
        //     .set_staking_requirement(2_000_000 * NOLAN_PER_SAITO, 60)
        //     .await;
        let issuance = vec![
            // (public_key.to_base58(), 100 * 2_000_000 * NOLAN_PER_SAITO),
            (public_key.to_base58(), 100_000 * NOLAN_PER_SAITO),
            (
                "27UK2MuBTdeARhYp97XBnCovGkEquJjkrQntCgYoqj6GC".to_string(),
                50_000 * NOLAN_PER_SAITO,
            ),
        ];
        tester.set_issuance(issuance).await.unwrap();
        tester.init().await.unwrap();
        tester.wait_till_block_id(1).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let genesis_period = tester
            .consensus_thread
            .config_lock
            .read()
            .await
            .get_consensus_config()
            .unwrap()
            .genesis_period;
        assert_eq!(genesis_period, 3, "genesis period should be 3");

        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(2).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");

        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(3).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
        {
            let blockchain = tester.consensus_thread.blockchain_lock.read().await;
            let block = blockchain.get_latest_block().expect("block should exist");
            assert_eq!(block.total_fees_atr, 0);
        }
        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(4).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
        {
            let blockchain = tester.consensus_thread.blockchain_lock.read().await;
            let block = blockchain.get_latest_block().expect("block should exist");
            assert!(block.total_fees_atr > 0);
        }

        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(5).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
        {
            let blockchain = tester.consensus_thread.blockchain_lock.read().await;
            let block = blockchain.get_latest_block().expect("block should exist");
            // assert!(block.total_fees_atr > 0);
        }

        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(6).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
        {
            let blockchain = tester.consensus_thread.blockchain_lock.read().await;
            let block = blockchain.get_latest_block().expect("block should exist");
            assert!(block.total_fees_atr > 0);
        }

        let tx = tester
            .create_transaction(1000, 1000, public_key)
            .await
            .unwrap();
        tester.add_transaction(tx).await;
        tester.wait_till_block_id(7).await.unwrap();
        tester
            .check_total_supply()
            .await
            .expect("total supply should not change");
        {
            let blockchain = tester.consensus_thread.blockchain_lock.read().await;
            let block = blockchain.get_latest_block().expect("block should exist");
            assert!(block.total_fees_atr > 0);
        }
    }
}

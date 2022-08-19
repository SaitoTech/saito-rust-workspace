use std::collections::VecDeque;
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
use crate::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

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
    pub generate_genesis_block: bool,
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
            log_read_lock_request!("wallet");
            let wallet = wallet_lock_clone.read().await;
            log_read_lock_receive!("wallet");
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        log_write_lock_request!("mempool");
        let mut mempool = mempool_lock_clone.write().await;
        log_write_lock_receive!("mempool");
        log_read_lock_request!("blockchain");
        let blockchain = blockchain_lock_clone.read().await;
        log_read_lock_receive!("blockchain");

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
        // trace!("processing new interface event");

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
            log_read_lock_request!("mempool");
            let mempool = self.mempool.read().await;
            log_read_lock_receive!("mempool");

            can_bundle = mempool
                .can_bundle_block(
                    self.blockchain.clone(),
                    timestamp,
                    self.generate_genesis_block,
                )
                .await;
            self.block_producing_timer = 0;
            work_done = true;
        }

        if can_bundle {
            {
                log_write_lock_request!("mempool");
                let mut mempool = self.mempool.write().await;
                log_write_lock_receive!("mempool");
                log_write_lock_request!("blockchain");
                let mut blockchain = self.blockchain.write().await;
                log_write_lock_receive!("blockchain");

                let block = mempool
                    .bundle_block(blockchain.deref_mut(), timestamp)
                    .await;
                debug!("adding bundled block to mempool");
                mempool.add_block(block);
            }

            add_to_blockchain_from_mempool(
                self.mempool.clone(),
                self.blockchain.clone(),
                &self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
            )
            .await;

            debug!("blocks added to blockchain");

            work_done = true;
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
                log_write_lock_request!("mempool");
                let mut mempool = self.mempool.write().await;
                log_write_lock_receive!("mempool");
                mempool.add_golden_ticket(golden_ticket).await;
                Some(())
            }
            ConsensusEvent::BlockFetched { peer_index, buffer } => {
                let mut block = Block::deserialize_from_net(&buffer);
                {
                    log_read_lock_request!("peers");
                    let peers = self.network.peers.read().await;
                    log_read_lock_receive!("peers");
                    log_read_lock_request!("blockchain");
                    let blockchain = self.blockchain.read().await;
                    log_read_lock_receive!("blockchain");
                    log_write_lock_request!("mempool");
                    let mut mempool = self.mempool.write().await;
                    log_write_lock_receive!("mempool");

                    let peer = peers.index_to_peers.get(&peer_index);
                    if peer.is_some() {
                        let peer = peer.unwrap();
                        block.source_connection_id = Some(peer.peer_public_key);
                    }

                    block.generate();

                    debug!("block : {:?} fetched from peer", hex::encode(block.hash));

                    if blockchain.blocks.contains_key(&block.hash) {
                        debug!(
                            "fetched block : {:?} already in blockchain",
                            hex::encode(block.hash)
                        );
                        return Some(());
                    }
                    debug!("adding fetched block to mempool");
                    mempool.add_block(block);
                }
                add_to_blockchain_from_mempool(
                    self.mempool.clone(),
                    self.blockchain.clone(),
                    &self.network,
                    &mut self.storage,
                    self.sender_to_miner.clone(),
                )
                .await;
                Some(())
            }
            ConsensusEvent::NewTransaction { mut transaction } => {
                transaction.generate_hash_for_signature();
                debug!(
                    "tx received with sig: {:?}",
                    hex::encode(transaction.signature)
                );
                log_write_lock_request!("mempool");
                let mut mempool = self.mempool.write().await;
                log_write_lock_receive!("mempool");
                mempool.add_transaction(transaction.clone()).await;
                self.network.propagate_transaction(&transaction).await;

                Some(())
            }
        };
    }

    async fn on_init(&mut self) {
        debug!("on_init");
        self.storage
            .load_blocks_from_disk(self.mempool.clone())
            .await;

        add_to_blockchain_from_mempool(
            self.mempool.clone(),
            self.blockchain.clone(),
            &self.network,
            &mut self.storage,
            self.sender_to_miner.clone(),
        )
        .await;
    }
}

pub async fn add_to_blockchain_from_mempool(
    mempool: Arc<RwLock<Mempool>>,
    blockchain: Arc<RwLock<Blockchain>>,
    network: &Network,
    storage: &mut Storage,
    sender_to_miner: Sender<MiningEvent>,
) {
    debug!("adding blocks from mempool to blockchain");
    let mut blocks: VecDeque<Block>;
    {
        log_write_lock_request!("mempool");
        let mut mempool = mempool.write().await;
        log_write_lock_receive!("mempool");
        blocks = mempool.blocks_queue.drain(..).collect();
    }
    blocks.make_contiguous().sort_by(|a, b| a.id.cmp(&b.id));

    // trace!("waiting for the log_write_lock_request!("blockchain");");
    log_write_lock_request!("blockchain");
    let mut blockchain = blockchain.write().await;
    log_write_lock_receive!("blockchain");
    // trace!("acquired the log_write_lock_request!("blockchain");");

    debug!("blocks to add : {:?}", blocks.len());
    while let Some(block) = blocks.pop_front() {
        blockchain
            .add_block(
                block,
                network,
                storage,
                sender_to_miner.clone(),
                mempool.clone(),
            )
            .await;
    }
}

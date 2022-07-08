use log::{debug, trace};
use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_event_processor::MiningEvent;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::time::interval;

#[derive(Debug)]
pub enum ConsensusEvent {
    NewGoldenTicket { golden_ticket: GoldenTicket },
    BlockFetched { peer_index: u64, buffer: Vec<u8> },
    CanBundleBlock { timestamp: u64 },
}

/// Manages blockchain and the mempool
pub struct ConsensusHandler {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub network: Network,
    pub storage: Storage,
}

impl ConsensusHandler {
    async fn run(
        mut handler: ConsensusHandler,
        mut event_sender: Sender<ConsensusEvent>,
        mut event_receiver: Receiver<ConsensusEvent>,
    ) {
        tokio::spawn(async move {
            handler.load_blocks_from_disk().await;

            ConsensusHandler::run_block_producer(
                handler.mempool.clone(),
                handler.blockchain.clone(),
                event_sender,
            )
            .await;

            ConsensusHandler::run_event_loop(handler, event_receiver).await;
        });
    }

    async fn load_blocks_from_disk(&mut self) {
        self.storage
            .load_blocks_from_disk(
                self.blockchain.clone(),
                &self.network,
                self.sender_to_miner.clone(),
            )
            .await;
    }

    async fn run_block_producer(
        mempool: Arc<RwLock<Mempool>>,
        blockchain: Arc<RwLock<Blockchain>>,
        mut event_sender: Sender<ConsensusEvent>,
    ) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;

                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let can_bundle;
                {
                    can_bundle = mempool
                        .read()
                        .await
                        .can_bundle_block(blockchain.clone(), timestamp)
                        .await;
                }

                if can_bundle {
                    event_sender
                        .send(ConsensusEvent::CanBundleBlock { timestamp })
                        .await
                        .expect("sending to mempool failed");
                }
            }
        });
    }

    async fn run_event_loop(
        mut handler: ConsensusHandler,
        mut event_receiver: Receiver<ConsensusEvent>,
    ) {
        loop {
            let result = event_receiver.recv().await;
            if result.is_some() {
                let event = result.unwrap();
                handler.process_event(event).await
            }
        }
    }

    async fn process_event(&mut self, event: ConsensusEvent) {
        match event {
            ConsensusEvent::NewGoldenTicket { golden_ticket } => {
                debug!(
                    "received new golden ticket : {:?}",
                    hex::encode(golden_ticket.target)
                );

                let mut mempool = self.mempool.write().await;
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

            ConsensusEvent::CanBundleBlock { .. } => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let mut mempool = self.mempool.write().await;
                let mut blockchain = self.blockchain.write().await;

                let result = mempool.bundle_block(&mut blockchain, timestamp).await;

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
            }
        }
    }
}

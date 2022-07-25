use crate::saito::rust_io_handler::RustIOHandler;
use log::{debug, info, trace};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::context::Context;
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;

#[derive(Debug)]
pub enum MempoolEvent {
    NewGoldenTicket { golden_ticket: GoldenTicket },
    CanBundleBlock { timestamp: u64 },
}

/// Manages blockchain and the mempool
pub struct MempoolHandler {
    mempool: Arc<RwLock<Mempool>>,
    blockchain: Arc<RwLock<Blockchain>>,
    sender_to_miner: Sender<MiningEvent>,
    event_sender: Sender<MempoolEvent>,
    event_receiver: Receiver<MempoolEvent>,
    network: Network,
    storage: Storage,
}

impl MempoolHandler {
    pub fn new(
        context: &Context,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_miner: Sender<MiningEvent>,
        event_sender: Sender<MempoolEvent>,
        event_receiver: Receiver<MempoolEvent>,
    ) -> Self {
        MempoolHandler {
            mempool: context.mempool.clone(),
            blockchain: context.blockchain.clone(),
            sender_to_miner,
            event_sender,
            event_receiver,
            network: Network::new(Box::new(RustIOHandler::new()), peers),
            storage: Storage::new(Box::new(RustIOHandler::new())),
        }
    }

    pub async fn run(mut handler: MempoolHandler) -> JoinHandle<()> {
        tokio::spawn(async move {
            handler.load_blocks_from_disk().await;

            MempoolHandler::run_block_producer(
                handler.mempool.clone(),
                handler.blockchain.clone(),
                handler.event_sender.clone(),
            )
            .await;

            info!("Mempool Handler is Running");

            handler.run_event_loop().await;
        })
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

    async fn run_event_loop(&mut self) {
        loop {
            if let Some(event) = self.event_receiver.recv().await {
                match event {
                    MempoolEvent::NewGoldenTicket { golden_ticket } => {
                        self.on_new_golden_ticket(golden_ticket).await;
                    }

                    MempoolEvent::CanBundleBlock { .. } => {
                        self.on_bundle_block().await;
                    }
                }
            }
        }
    }

    async fn run_block_producer(
        mempool: Arc<RwLock<Mempool>>,
        blockchain: Arc<RwLock<Blockchain>>,
        event_sender: Sender<MempoolEvent>,
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
                        .send(MempoolEvent::CanBundleBlock { timestamp })
                        .await
                        .expect("sending to mempool failed");
                }
            }
        });
    }

    async fn on_bundle_block(&mut self) {
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

    async fn on_new_golden_ticket(&mut self, golden_ticket: GoldenTicket) {
        debug!(
            "received new golden ticket : {:?}",
            hex::encode(golden_ticket.target)
        );

        let mut mempool = self.mempool.write().await;
        mempool.add_golden_ticket(golden_ticket).await;
    }
}

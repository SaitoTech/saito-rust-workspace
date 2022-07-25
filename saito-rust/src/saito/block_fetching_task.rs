use crate::saito::rust_io_handler::RustIOHandler;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

pub struct BlockFetchingTaskRunner {
    pub peers: Arc<RwLock<PeerCollection>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_miner: Sender<MiningEvent>,
}

impl BlockFetchingTaskRunner {
    pub async fn run_task(&self, url: String) -> JoinHandle<()> {
        let mut task = BlockFetchingTask::new(
            url,
            self.blockchain.clone(),
            self.peers.clone(),
            self.sender_to_miner.clone(),
        );
        tokio::spawn(async move {
            task.run().await;
        })
    }
}

pub struct BlockFetchingTask {
    url: String,
    blockchain: Arc<RwLock<Blockchain>>,
    network: Network,
    storage: Storage,
    sender_to_miner: Sender<MiningEvent>,
}

impl BlockFetchingTask {
    pub fn new(
        url: String,
        blockchain: Arc<RwLock<Blockchain>>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_miner: Sender<MiningEvent>,
    ) -> Self {
        BlockFetchingTask {
            url,
            blockchain,
            network: Network::new(Box::new(RustIOHandler::new()), peers),
            storage: Storage::new(Box::new(RustIOHandler::new())),
            sender_to_miner,
        }
    }

    async fn run(&mut self) {
        let result = self
            .network
            .fetch_missing_block_by_url(self.url.clone())
            .await;

        match result {
            Ok(block) => {
                let mut blockchain = self.blockchain.write().await;
                blockchain.add_block(
                    block,
                    &self.network,
                    &mut self.storage,
                    self.sender_to_miner.clone(),
                );
            }
            Err(_error) => {
                todo!("handle block fetch error")
            }
        }
    }
}

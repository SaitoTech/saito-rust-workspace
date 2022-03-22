use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::mempool_controller::MempoolEvent;
use crate::core::miner_controller::MinerEvent;

pub enum BlockchainEvent {
    NewBlockBundled(Block),
    BlockFetched(Vec<u8>),
}

pub struct BlockchainController {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_handler: Box<dyn HandleIo + Send + Sync>,
}

impl BlockchainController {}

#[async_trait]
impl ProcessEvent<BlockchainEvent> for BlockchainController {
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
        None
    }

    async fn process_event(&mut self, event: BlockchainEvent) -> Option<()> {
        trace!("processing blockchain event");

        match event {
            BlockchainEvent::NewBlockBundled(block) => {
                let mut blockchain = self.blockchain.write().await;
                blockchain
                    .add_block(block, &self.io_handler, self.peers.clone())
                    .await;
            }
            BlockchainEvent::BlockFetched(buffer) => {}
        }
        None
    }
}

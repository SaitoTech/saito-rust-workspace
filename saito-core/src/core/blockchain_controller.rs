use std::borrow::Borrow;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::defs::SaitoHash;
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockchain::Blockchain;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::storage::Storage;
use crate::core::mempool_controller::MempoolEvent;
use crate::core::miner_controller::MinerEvent;

pub enum BlockchainEvent {
    NewBlockBundled(Block),
}

pub struct BlockchainController {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_handler: Box<dyn HandleIo + Send + Sync>,
}

impl BlockchainController {
    async fn propagate_block_to_peers(&self, block_hash: SaitoHash) {
        debug!("propagating blocks to peers");
        let peers = self.peers.read().await;
        let blockchain = self.blockchain.write().await;
        let block = blockchain.blocks.get(&block_hash);
        if block.is_none() {
            // TODO : handle
        }
        let block = block.unwrap();
        let buffer = block.serialize_for_net(BlockType::Full);
        let mut exceptions = vec![];

        // finding block sender to avoid resending the block to that node
        if block.source_connection_id.is_some() {
            let peers = self.peers.read().await;
            let peer = peers
                .address_to_peers
                .get(&block.source_connection_id.unwrap());
            if peer.is_some() {
                exceptions.push(peer.unwrap().peer_index);
            }
        }

        self.io_handler
            .send_message_to_all("BLOCK".parse().unwrap(), buffer, exceptions);
    }
}

#[async_trait]
impl ProcessEvent<BlockchainEvent> for BlockchainController {
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        debug!("processing new interface event");
        match event {
            InterfaceEvent::OutgoingNetworkMessage {
                peer_index: _,
                message_name: _,
                buffer: _,
            } => {}
            InterfaceEvent::IncomingNetworkMessage {
                peer_index: _,
                message_name: _,
                buffer: _,
            } => {}
            InterfaceEvent::DataSaveResponse {
                key: key,
                result: result,
            } => {
                // propagate block to network
                // TODO : add a data type == block check here
                let hash: SaitoHash = hex::decode(key).unwrap().try_into().unwrap();
                self.propagate_block_to_peers(hash);
                // self.io_handler.set_write_result(index, result)
            }
            InterfaceEvent::DataReadRequest(_) => {}
            InterfaceEvent::DataReadResponse(_, _, _) => {}
            InterfaceEvent::ConnectToPeer(_) => {}
            InterfaceEvent::PeerConnected(_, _) => {}
            InterfaceEvent::PeerDisconnected(_) => {}
            InterfaceEvent::BlockFetchRequest(_, _) => {}
            _ => {
                unreachable!()
            }
        }
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
                    .add_block(block, &mut self.io_handler, self.peers.clone())
                    .await;
            }
        }
        None
    }
}

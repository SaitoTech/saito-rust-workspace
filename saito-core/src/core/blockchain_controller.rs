use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::defs::SaitoHash;
use crate::common::handle_io::HandleIo;
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::data;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::storage::Storage;
use crate::core::mempool_controller::MempoolEvent;
use crate::core::miner_controller::MinerEvent;

pub enum BlockchainEvent {
    NewBlockBundled(Block),
}

pub enum PeerState {
    Connected,
    Connecting,
    Disconnected,
}

pub struct StaticPeer {
    pub peer_details: data::configuration::Peer,
    pub peer_state: PeerState,
    pub peer_index: u64,
}

pub struct BlockchainController {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub static_peers: Vec<StaticPeer>,
    pub configs: Arc<RwLock<Configuration>>,
    pub io_handler: Box<dyn HandleIo + Send + Sync>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
}

impl BlockchainController {
    async fn propagate_block_to_peers(&self, block_hash: SaitoHash) {
        debug!("propagating blocks to peers");
        let peers = self.peers.read().await;
        let buffer: Vec<u8>;
        let mut exceptions = vec![];
        {
            let blockchain = self.blockchain.write().await;
            let block = blockchain.blocks.get(&block_hash);
            if block.is_none() {
                // TODO : handle
            }
            let block = block.unwrap();
            buffer = block.serialize_for_net(BlockType::Full);

            // finding block sender to avoid resending the block to that node
            if block.source_connection_id.is_some() {
                let peers = self.peers.read().await;
                let peer = peers
                    .address_to_peers
                    .get(&block.source_connection_id.unwrap());
                if peer.is_some() {
                    exceptions.push(*peer.unwrap());
                }
            }
        }

        self.io_handler
            .send_message_to_all("BLOCK".parse().unwrap(), buffer, exceptions)
            .await;
        debug!("block sent to peers");
    }
    async fn connect_to_static_peers(&mut self) {
        debug!("connect to peers from config");
        let mut configs = self.configs.write().await;

        for peer in &mut configs.peers {
            self.io_handler.connect_to_peer(peer.clone()).await;
        }
    }
    async fn handle_new_peer(&mut self, peer: Option<data::configuration::Peer>, peer_index: u64) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        let mut peers = self.peers.write().await;
        // for mut static_peer in &mut self.static_peers {
        //     if static_peer.peer_details == peer {
        //         static_peer.peer_state = PeerState::Connected;
        //     }
        // }
        let mut peer = Peer::new(peer_index);
        peer.initiate_handshake(&self.io_handler).await;

        peers.index_to_peers.insert(peer_index, peer);
        info!("new peer added : {:?}", peer_index);
    }
    async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        todo!()
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
            } => {
                // TODO : remove this case if not being used
            }
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
                self.propagate_block_to_peers(hash).await;
                // self.io_handler.set_write_result(index, result)
            }
            InterfaceEvent::DataReadResponse(_, _, _) => {}
            InterfaceEvent::PeerConnectionResult {
                peer_details,
                result,
            } => {
                if result.is_ok() {
                    self.handle_new_peer(peer_details, result.unwrap()).await;
                }
            }
            InterfaceEvent::PeerDisconnected { peer_index } => {}
            _ => {
                unreachable!()
            }
        }
        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        None
    }

    async fn process_event(&mut self, event: BlockchainEvent) -> Option<()> {
        debug!("processing blockchain event");

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

    async fn on_init(&mut self) {
        // connect to peers
        self.connect_to_static_peers().await;
    }
}

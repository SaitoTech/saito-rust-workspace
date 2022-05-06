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
use crate::core::data::msg::message::Message;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::storage::Storage;
use crate::core::data::wallet::Wallet;
use crate::core::mempool_controller::MempoolEvent;
use crate::core::miner_controller::MinerEvent;

#[derive(Debug)]
pub enum BlockchainEvent {
    NewBlockBundled(Block),
}

#[derive(Debug)]
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
    pub wallet: Arc<RwLock<Wallet>>,
}

impl BlockchainController {
    async fn propagate_block_to_peers(&self, block_hash: SaitoHash) {
        debug!("propagating blocks to peers");
        let buffer: Vec<u8>;
        let mut exceptions = vec![];
        {
            trace!("waiting for the blockchain write lock");
            let blockchain = self.blockchain.read().await;
            trace!("acquired the blockchain write lock");
            let block = blockchain.blocks.get(&block_hash);
            if block.is_none() {
                // TODO : handle
            }
            let block = block.unwrap();
            buffer = block.serialize_for_net(BlockType::Header);

            // finding block sender to avoid resending the block to that node
            if block.source_connection_id.is_some() {
                trace!("waiting for the peers read lock");
                let peers = self.peers.read().await;
                trace!("acquired the peers read lock");
                let peer = peers
                    .address_to_peers
                    .get(&block.source_connection_id.unwrap());
                if peer.is_some() {
                    exceptions.push(*peer.unwrap());
                }
            }
        }

        self.io_handler
            .send_message_to_all(buffer, exceptions)
            .await
            .unwrap();
        debug!("block sent to peers");
    }
    async fn connect_to_static_peers(&mut self) {
        debug!("connect to peers from config",);
        trace!("waiting for the configs read lock");
        let configs = self.configs.read().await;
        trace!("acquired the configs read lock");

        for peer in &configs.peers {
            self.io_handler.connect_to_peer(peer.clone()).await.unwrap();
        }
        debug!("connected to peers");
    }
    async fn handle_new_peer(
        &mut self,
        peer_data: Option<data::configuration::Peer>,
        peer_index: u64,
    ) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        trace!("waiting for the peers write lock");
        let mut peers = self.peers.write().await;
        trace!("acquired the peers write lock");
        // for mut static_peer in &mut self.static_peers {
        //     if static_peer.peer_details == peer {
        //         static_peer.peer_state = PeerState::Connected;
        //     }
        // }
        let mut peer = Peer::new(peer_index);
        if peer_data.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(&self.io_handler, self.wallet.clone())
                .await
                .unwrap();
        }

        peers.index_to_peers.insert(peer_index, peer);
        info!("new peer added : {:?}", peer_index);

        // start block syncing here
    }

    async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        todo!()
    }
    async fn request_blockchain_from_peer(&mut self, peer_index: u64) {
        debug!("requesting blockchain from peer : {:?}", peer_index);
        todo!()
    }
    async fn process_incoming_message(&mut self, peer_index: u64, message: Message) {
        debug!(
            "processing incoming message type : {:?} from peer : {:?}",
            message, peer_index
        );
        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                let mut peers = self.peers.write().await;
                let peer = peers.index_to_peers.get_mut(&peer_index);
                if peer.is_none() {
                    todo!()
                }
                let peer = peer.unwrap();
                peer.handle_handshake_challenge(challenge, &self.io_handler, self.wallet.clone())
                    .await
                    .unwrap();
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                let mut peers = self.peers.write().await;
                let peer = peers.index_to_peers.get_mut(&peer_index);
                if peer.is_none() {
                    todo!()
                }
                let peer = peer.unwrap();
                peer.handle_handshake_response(response, &self.io_handler, self.wallet.clone())
                    .await
                    .unwrap();
                if peer.handshake_done {
                    // TODO :sync block data from peer
                }
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                let mut peers = self.peers.write().await;
                let peer = peers.index_to_peers.get_mut(&peer_index);
                if peer.is_none() {
                    todo!()
                }
                let peer = peer.unwrap();
                let result = peer
                    .handle_handshake_completion(response, &self.io_handler)
                    .await;
                if peer.handshake_done {
                    // TODO :sync block data from peer
                }
            }
            Message::ApplicationMessage(_) => {
                debug!("received buffer");
            }
            Message::Block(_) => {
                debug!("received block");
            }
            Message::Transaction(_) => {
                debug!("received transaction");
            }
            Message::BlockchainRequest(_) => {}
        }
    }
}

#[async_trait]
impl ProcessEvent<BlockchainEvent> for BlockchainController {
    async fn process_global_event(&mut self, _event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        debug!("processing new interface event");
        match event {
            InterfaceEvent::OutgoingNetworkMessage { peer_index, buffer } => {
                // TODO : remove this case if not being used
                unreachable!()
            }
            InterfaceEvent::IncomingNetworkMessage { peer_index, buffer } => {
                debug!("incoming message received from peer : {:?}", peer_index);
                let message = Message::deserialize(buffer);
                if message.is_err() {
                    todo!()
                }
                self.process_incoming_message(peer_index, message.unwrap())
                    .await;
            }
            InterfaceEvent::PeerConnectionResult {
                peer_details,
                result,
            } => {
                if result.is_ok() {
                    self.handle_new_peer(peer_details, result.unwrap()).await;
                }
            }
            InterfaceEvent::PeerDisconnected { .. } => {}
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
                trace!("waiting for the blockchain write lock");
                {
                    let mut blockchain = self.blockchain.write().await;
                    trace!("acquired the blockchain write lock");
                    blockchain
                        .add_block(
                            block,
                            &mut self.io_handler,
                            self.peers.clone(),
                            self.sender_to_miner.clone(),
                        )
                        .await;
                }
            }
        }

        debug!("blockchain event processed successfully");
        None
    }

    async fn on_init(&mut self) {
        debug!("on_init");
        {
            Storage::load_blocks_from_disk(
                self.blockchain.clone(),
                &mut self.io_handler,
                self.peers.clone(),
                self.sender_to_miner.clone(),
            )
            .await;
        }
        // connect to peers
        self.connect_to_static_peers().await;
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn process_new_transaction() {}
}

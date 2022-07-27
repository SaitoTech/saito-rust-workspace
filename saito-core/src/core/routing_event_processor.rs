use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::SaitoHash;
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::consensus_event_processor::ConsensusEvent;
use crate::core::data;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::message::Message;
use crate::core::data::network::Network;
use crate::core::data::wallet::Wallet;
use crate::core::mining_event_processor::MiningEvent;

#[derive(Debug)]
pub enum RoutingEvent {}

#[derive(Debug)]
pub enum PeerState {
    Connected,
    Connecting,
    Disconnected,
}

pub struct StaticPeer {
    pub peer_details: data::configuration::PeerConfig,
    pub peer_state: PeerState,
    pub peer_index: u64,
}

/// Manages peers and routes messages to correct controller
pub struct RoutingEventProcessor {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<ConsensusEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    // TODO : remove this if not needed
    pub static_peers: Vec<StaticPeer>,
    pub configs: Arc<RwLock<Configuration>>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub network: Network,
}

impl RoutingEventProcessor {
    ///
    ///
    /// # Arguments
    ///
    /// * `peer_index`:
    /// * `message`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn process_incoming_message(&mut self, peer_index: u64, message: Message) {
        debug!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            peer_index
        );
        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                // self.network
                //     .handle_handshake_challenge(
                //         peer_index,
                //         challenge,
                //         self.wallet.clone(),
                //         self.configs.clone(),
                //     )
                //     .await;
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                // self.network
                //     .handle_handshake_response(
                //         peer_index,
                //         response,
                //         self.wallet.clone(),
                //         self.blockchain.clone(),
                //     )
                //     .await;
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                // self.network
                //     .handle_handshake_completion(peer_index, response, self.blockchain.clone())
                //     .await;
            }
            Message::ApplicationMessage(_) => {
                debug!("received buffer");
            }
            Message::Block(_) => {
                debug!("received block");
            }
            Message::Transaction(transaction) => {
                debug!("received transaction");

                self.sender_to_mempool
                    .send(ConsensusEvent::NewTransaction { transaction })
                    .await
                    .unwrap();
            }
            Message::BlockchainRequest(request) => {}
            Message::BlockHeaderHash(hash) => {}
        }
        debug!("incoming message processed");
    }

    async fn connect_to_static_peers(&mut self) {
        // debug!("connect to peers from config",);
        // self.network
        //     .connect_to_static_peers(self.configs.clone())
        //     .await;
    }
    async fn handle_new_peer(
        &mut self,
        peer_data: Option<data::configuration::PeerConfig>,
        peer_index: u64,
    ) {
        trace!("handling new peer : {:?}", peer_index);
        // self.network
        //     .handle_new_peer(
        //         peer_data,
        //         peer_index,
        //         self.wallet.clone(),
        //         self.configs.clone(),
        //     )
        //     .await;
    }

    async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        //self.network.handle_peer_disconnect(peer_index).await;
    }
}

#[async_trait]
impl ProcessEvent<RoutingEvent> for RoutingEventProcessor {
    async fn process_network_event(&mut self, event: NetworkEvent) -> Option<()> {
        debug!("processing new interface event");
        match event {
            NetworkEvent::OutgoingNetworkMessage {
                peer_index: _,
                buffer: _,
            } => {
                // TODO : remove this case if not being used
                unreachable!()
            }
            // NetworkEvent::IncomingNetworkMessage { peer_index, buffer } => {
            //     debug!("incoming message received from peer : {:?}", peer_index);
            //     let message = Message::deserialize(buffer);
            //     if message.is_err() {
            //         todo!()
            //     }
            //     self.process_incoming_message(peer_index, message.unwrap())
            //         .await;
            // }
            // NetworkEvent::PeerConnectionResult {
            //     peer_details,
            //     result,
            // } => {
            //     if result.is_ok() {
            //         self.handle_new_peer(peer_details, result.unwrap()).await;
            //     }
            // }
            // NetworkEvent::PeerDisconnected { peer_index } => {
            //     self.handle_peer_disconnect(peer_index).await;
            // }
            //
            // NetworkEvent::OutgoingNetworkMessageForAll { .. } => {
            //     unreachable!()
            // }
            // NetworkEvent::ConnectToPeer { .. } => {
            //     unreachable!()
            // }
            NetworkEvent::BlockFetchRequest { .. } => {
                unreachable!()
            }
            NetworkEvent::PeerDisconnected { peer_index } => {
                unreachable!()
            }
            NetworkEvent::BlockFetched {
                block_hash,
                peer_index,
                buffer,
                request_id,
            } => {
                debug!("block received : {:?}", hex::encode(block_hash));
                self.sender_to_mempool
                    .send(ConsensusEvent::BlockFetched { peer_index, buffer })
                    .await
                    .unwrap();
            }
        }
        None
    }
    async fn process_timer_event(&mut self, _duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        None
    }

    async fn process_event(&mut self, _event: RoutingEvent) -> Option<()> {
        debug!("processing blockchain event");

        // match event {}

        debug!("blockchain event processed successfully");
        None
    }

    async fn on_init(&mut self) {
        // connect to peers
        self.connect_to_static_peers().await;
    }
}

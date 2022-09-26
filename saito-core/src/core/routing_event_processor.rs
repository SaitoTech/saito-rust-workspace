use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tracing::{debug, trace};

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoHash, StatVariable, Timestamp, STAT_BIN_COUNT};
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
use crate::{log_read_lock_receive, log_read_lock_request};

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

pub struct RoutingStats {
    pub received_transactions: StatVariable,
    pub received_blocks: StatVariable,
    pub total_incoming_messages: StatVariable,
}

impl Default for RoutingStats {
    fn default() -> Self {
        RoutingStats {
            received_transactions: StatVariable::new(
                "routing::received_txs".to_string(),
                STAT_BIN_COUNT,
            ),
            received_blocks: StatVariable::new(
                "routing::received_blocks".to_string(),
                STAT_BIN_COUNT,
            ),
            total_incoming_messages: StatVariable::new(
                "routing::incoming_msgs".to_string(),
                STAT_BIN_COUNT,
            ),
        }
    }
}

/// Manages peers and routes messages to correct controller
pub struct RoutingEventProcessor {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<ConsensusEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    // TODO : remove this if not needed
    pub static_peers: Vec<StaticPeer>,
    pub configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub network: Network,
    pub reconnection_timer: Timestamp,
    pub stats: RoutingStats,
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
    #[tracing::instrument(level = "info", skip_all)]
    async fn process_incoming_message(&mut self, peer_index: u64, message: Message) {
        trace!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            peer_index
        );

        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                self.network
                    .handle_handshake_challenge(
                        peer_index,
                        challenge,
                        self.wallet.clone(),
                        self.configs.clone(),
                    )
                    .await;
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                self.network
                    .handle_handshake_response(
                        peer_index,
                        response,
                        self.wallet.clone(),
                        self.blockchain.clone(),
                    )
                    .await;
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                self.network
                    .handle_handshake_completion(peer_index, response, self.blockchain.clone())
                    .await;
            }
            Message::ApplicationMessage(_) => {
                debug!("received buffer");
            }
            Message::Block(_) => {
                unreachable!("received block");
            }
            Message::Transaction(transaction) => {
                debug!("received transaction");
                self.stats.received_transactions.increment();
                self.sender_to_mempool
                    .send(ConsensusEvent::NewTransaction { transaction })
                    .await
                    .unwrap();
            }
            Message::BlockchainRequest(request) => {
                self.process_incoming_blockchain_request(request, peer_index)
                    .await;
            }
            Message::BlockHeaderHash(hash) => {
                self.process_incoming_block_hash(hash, peer_index).await;
            }
            Message::Ping() => {}
            Message::SPVChain() => {}
            Message::Services() => {}
            Message::GhostChain() => {}
            Message::GhostChainRequest() => {}
            Message::Result() => {}
            Message::Error() => {}
        }
        trace!("incoming message processed");
    }

    #[tracing::instrument(level = "info", skip_all)]
    async fn handle_new_peer(
        &mut self,
        peer_data: Option<data::configuration::PeerConfig>,
        peer_index: u64,
    ) {
        trace!("handling new peer : {:?}", peer_index);
        self.network
            .handle_new_peer(
                peer_data,
                peer_index,
                self.wallet.clone(),
                self.configs.clone(),
            )
            .await;
    }

    #[tracing::instrument(level = "info", skip_all)]
    async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        self.network.handle_peer_disconnect(peer_index).await;
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        peer_index: u64,
    ) {
        debug!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            hex::encode(request.latest_block_hash),
            hex::encode(request.fork_id),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        log_read_lock_request!("blockchain");
        let blockchain = self.blockchain.read().await;
        log_read_lock_receive!("blockchain");

        let last_shared_ancestor =
            blockchain.generate_last_shared_ancestor(request.latest_block_id, request.fork_id);
        debug!("last shared ancestor = {:?}", last_shared_ancestor);

        for i in last_shared_ancestor..(blockchain.blockring.get_latest_block_id() + 1) {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i);
            if block_hash == [0; 32] {
                // TODO : can the block hash not be in the ring if we are going through the longest chain ?
                continue;
            }
            let buffer = Message::BlockHeaderHash(block_hash).serialize();
            self.network
                .io_interface
                .send_message(peer_index, buffer)
                .await
                .unwrap();
        }
    }
    #[tracing::instrument(level = "info", skip_all)]
    async fn process_incoming_block_hash(&self, block_hash: SaitoHash, peer_index: u64) {
        debug!(
            "processing incoming block hash : {:?} from peer : {:?}",
            hex::encode(block_hash),
            peer_index
        );
        self.network
            .process_incoming_block_hash(block_hash, peer_index, self.blockchain.clone())
            .await;
    }
}

#[async_trait]
impl ProcessEvent<RoutingEvent> for RoutingEventProcessor {
    async fn process_network_event(&mut self, event: NetworkEvent) -> Option<()> {
        // trace!("processing new interface event");
        match event {
            NetworkEvent::OutgoingNetworkMessage {
                peer_index: _,
                buffer: _,
            } => {
                // TODO : remove this case if not being used
                unreachable!()
            }
            NetworkEvent::IncomingNetworkMessage { peer_index, buffer } => {
                trace!("incoming message received from peer : {:?}", peer_index);
                let message = Message::deserialize(buffer);
                if message.is_err() {
                    //todo!()
                    return None;
                }

                self.stats.total_incoming_messages.increment();
                self.process_incoming_message(peer_index, message.unwrap())
                    .await;
                return Some(());
            }
            NetworkEvent::PeerConnectionResult {
                peer_details,
                result,
            } => {
                if result.is_ok() {
                    self.handle_new_peer(peer_details, result.unwrap()).await;
                    return Some(());
                }
            }
            NetworkEvent::PeerDisconnected { peer_index } => {
                self.handle_peer_disconnect(peer_index).await;
                return Some(());
            }

            NetworkEvent::OutgoingNetworkMessageForAll { .. } => {
                unreachable!()
            }
            NetworkEvent::ConnectToPeer { .. } => {
                unreachable!()
            }
            NetworkEvent::BlockFetchRequest { .. } => {
                unreachable!()
            }
            NetworkEvent::BlockFetched {
                block_hash,
                peer_index,
                buffer,
            } => {
                debug!("block received : {:?}", hex::encode(block_hash));
                self.sender_to_mempool
                    .send(ConsensusEvent::BlockFetched { peer_index, buffer })
                    .await
                    .unwrap();
                return Some(());
            }
        }
        None
    }
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        let duration_value = duration.as_micros() as Timestamp;

        self.reconnection_timer = self.reconnection_timer + duration_value;
        // TODO : move the hard code value to a config
        if self.reconnection_timer >= 10_000_000 {
            self.network.connect_to_static_peers().await;
            self.reconnection_timer = 0;
        }

        None
    }

    async fn process_event(&mut self, _event: RoutingEvent) -> Option<()> {
        None
    }

    async fn on_init(&mut self) {
        // connect to peers
        self.network
            .initialize_static_peers(self.configs.clone())
            .await;
    }
    async fn on_stat_interval(&mut self) {
        let time = self.time_keeper.get_timestamp();
        self.stats.received_transactions.calculate_stats(time);
        self.stats.received_blocks.calculate_stats(time);
        self.stats.total_incoming_messages.calculate_stats(time);

        println!("------------ routing stats -------------");
        self.stats.received_transactions.print();
        self.stats.received_blocks.print();
        self.stats.total_incoming_messages.print();
        println!("---------- routing stats end -----------");
    }
}

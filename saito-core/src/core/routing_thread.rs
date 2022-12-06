use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tracing::{debug, info, trace};

use crate::common::command::NetworkEvent;
use crate::common::defs::{
    PeerIndex, SaitoHash, SaitoPublicKey, StatVariable, Timestamp, STAT_BIN_COUNT,
};
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::data;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::blockchain_sync_state::BlockchainSyncState;
use crate::core::data::configuration::Configuration;
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::message::Message;
use crate::core::data::network::Network;
use crate::core::data::wallet::Wallet;
use crate::core::mining_thread::MiningEvent;
use crate::core::verification_thread::VerifyRequest;
use crate::{log_read_lock_receive, log_read_lock_request};

#[derive(Debug)]
pub enum RoutingEvent {
    BlockchainUpdated,
}

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

impl RoutingStats {
    pub fn new(sender: Sender<String>) -> Self {
        RoutingStats {
            received_transactions: StatVariable::new(
                "routing::received_txs".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
            received_blocks: StatVariable::new(
                "routing::received_blocks".to_string(),
                STAT_BIN_COUNT,
                sender.clone(),
            ),
            total_incoming_messages: StatVariable::new(
                "routing::incoming_msgs".to_string(),
                STAT_BIN_COUNT,
                sender,
            ),
        }
    }
}

/// Manages peers and routes messages to correct controller
pub struct RoutingThread {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_consensus: Sender<ConsensusEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    // TODO : remove this if not needed
    pub static_peers: Vec<StaticPeer>,
    pub configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub network: Network,
    pub reconnection_timer: Timestamp,
    pub stats: RoutingStats,
    pub public_key: SaitoPublicKey,
    pub senders_to_verification: Vec<Sender<VerifyRequest>>,
    pub last_verification_thread_index: usize,
    pub stat_sender: Sender<String>,
    pub blockchain_sync_state: BlockchainSyncState,
}

impl RoutingThread {
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
    // #[tracing::instrument(level = "info", skip_all)]
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
                        self.configs.clone(),
                    )
                    .await;
            }
            Message::ApplicationMessage(_) => {
                debug!("received buffer");
            }
            Message::Block(_) => {
                unreachable!("received block");
            }
            Message::Transaction(transaction) => {
                trace!("received transaction");
                self.stats.received_transactions.increment();
                self.send_to_verification_thread(VerifyRequest::Transaction(transaction))
                    .await;
            }
            Message::BlockchainRequest(request) => {
                self.process_incoming_blockchain_request(request, peer_index)
                    .await;
            }
            Message::BlockHeaderHash(hash, prev_hash) => {
                self.process_incoming_block_hash(hash, prev_hash, peer_index)
                    .await;
            }
            Message::Ping() => {}
            Message::SPVChain() => {}
            Message::Services() => {}
            Message::GhostChain() => {}
            Message::GhostChainRequest() => {}
            Message::Result() => {}
            Message::Error() => {}
            Message::ApplicationTransaction(_) => {}
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
        self.network.handle_new_peer(peer_data, peer_index).await;
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
        info!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            hex::encode(request.latest_block_hash),
            hex::encode(request.fork_id),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        log_read_lock_request!("routing_thread:process_incoming_blockchain_request:blockchain");
        let blockchain = self.blockchain.read().await;
        log_read_lock_receive!("routing_thread:process_incoming_blockchain_request:blockchain");

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
            let buffer = Message::BlockHeaderHash(block_hash, i).serialize();
            self.network
                .io_interface
                .send_message(peer_index, buffer)
                .await
                .unwrap();
        }
    }
    // #[tracing::instrument(level = "info", skip_all)]
    async fn process_incoming_block_hash(
        &mut self,
        block_hash: SaitoHash,
        block_id: u64,
        peer_index: u64,
    ) {
        debug!(
            "processing incoming block hash : {:?} from peer : {:?}",
            hex::encode(block_hash),
            peer_index
        );

        self.blockchain_sync_state
            .add_entry(block_hash, block_id, peer_index);

        self.fetch_next_blocks().await;
    }
    // #[tracing::instrument(level = "info", skip_all)]
    async fn fetch_next_blocks(&mut self) {
        {
            log_read_lock_request!("RoutingThread:fetch_next_blocks::blockchain");
            let blockchain = self.blockchain.read().await;
            log_read_lock_receive!("RoutingThread:fetch_next_blocks::blockchain");
            self.blockchain_sync_state
                .set_latest_blockchain_id(blockchain.get_latest_block_id());
        }

        self.blockchain_sync_state.build_peer_block_picture();

        let map = self.blockchain_sync_state.request_blocks_from_waitlist();

        let mut fetched_blocks: Vec<(PeerIndex, SaitoHash)> = Default::default();
        for (peer_index, vec) in map {
            for hash in vec.iter() {
                let result = self
                    .network
                    .process_incoming_block_hash(*hash, peer_index, self.blockchain.clone())
                    .await;
                if result.is_some() {
                    fetched_blocks.push((peer_index, *hash));
                } else {
                    // if we already have the block added don't need to request it from peer
                    self.blockchain_sync_state.remove_entry(*hash, peer_index);
                }
            }
        }
        self.blockchain_sync_state.mark_as_fetching(fetched_blocks);
    }
    async fn send_to_verification_thread(&mut self, request: VerifyRequest) {
        // waiting till we get an acceptable sender
        let sender_count = self.senders_to_verification.len();
        let mut trials = 0;
        loop {
            trials += 1;
            self.last_verification_thread_index += 1;
            let sender_index: usize = self.last_verification_thread_index % sender_count;
            let sender = self
                .senders_to_verification
                .get(sender_index)
                .expect("sender should be here as we are using the modulus on index");

            if sender.capacity() > 0 {
                sender.send(request).await.unwrap();
                return;
            }
            if trials == sender_count {
                // if all the channels are full, we will sleep for a bit till some space is available
                tokio::time::sleep(Duration::from_millis(10)).await;
                trials = 0;
            }
        }
    }
}

#[async_trait]
impl ProcessEvent<RoutingEvent> for RoutingThread {
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

                self.send_to_verification_thread(VerifyRequest::Block(buffer, peer_index))
                    .await;

                self.blockchain_sync_state
                    .mark_as_fetched(peer_index, block_hash);

                self.fetch_next_blocks().await;

                return Some(());
            }
        }
        None
    }
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        let duration_value = duration.as_millis() as Timestamp;

        self.reconnection_timer += duration_value;
        // TODO : move the hard code value to a config
        if self.reconnection_timer >= 10_000 {
            self.network.connect_to_static_peers().await;
            self.reconnection_timer = 0;
        }

        None
    }

    async fn process_event(&mut self, event: RoutingEvent) -> Option<()> {
        match event {
            RoutingEvent::BlockchainUpdated => {
                debug!("received blockchain update event");
                self.fetch_next_blocks().await;
            }
        }
        None
    }

    async fn on_init(&mut self) {
        assert!(!self.senders_to_verification.is_empty());
        // connect to peers
        self.network
            .initialize_static_peers(self.configs.clone())
            .await;

        {
            log_read_lock_request!("RoutingThread:on_init::wallet");
            let wallet = self.wallet.read().await;
            log_read_lock_receive!("RoutingThread:on_init::wallet");
            self.public_key = wallet.public_key;
        }
    }
    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        self.stats
            .received_transactions
            .calculate_stats(current_time)
            .await;
        self.stats
            .received_blocks
            .calculate_stats(current_time)
            .await;
        self.stats
            .total_incoming_messages
            .calculate_stats(current_time)
            .await;

        let stat = format!(
            "--- stats ------ {} - capacity : {:?} / {:?}",
            format!("{:width$}", "consensus::queue", width = 30),
            self.sender_to_consensus.capacity(),
            self.sender_to_consensus.max_capacity()
        );
        self.stat_sender.send(stat).await.unwrap();
        for (index, sender) in self.senders_to_verification.iter().enumerate() {
            let stat = format!(
                "--- stats ------ {} - capacity : {:?} / {:?}",
                format!(
                    "{:width$}",
                    format!("verification_{:?}::queue", index),
                    width = 30
                ),
                sender.capacity(),
                sender.max_capacity()
            );
            self.stat_sender.send(stat).await.unwrap();
        }

        let stats = self.blockchain_sync_state.get_stats();
        for stat in stats {
            self.stat_sender.send(stat).await.unwrap();
        }
    }
}

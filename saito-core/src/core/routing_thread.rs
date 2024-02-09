use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace, warn};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::defs::{
    push_lock, BlockId, PeerIndex, PrintForLog, SaitoHash, StatVariable, Timestamp,
    LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_PEERS, STAT_BIN_COUNT,
};
use crate::common::keep_time::KeepTime;
use crate::common::network_event::NetworkEvent;
use crate::common::process_event::ProcessEvent;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::blockchain_sync_state::BlockchainSyncState;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::peer_service::PeerService;
use crate::core::consensus::wallet::Wallet;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::io::network::Network;
use crate::core::mining_thread::MiningEvent;
use crate::core::msg::block_request::BlockchainRequest;
use crate::core::msg::ghost_chain_sync::GhostChainSync;
use crate::core::msg::message::Message;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::hash;
use crate::core::verification_thread::VerifyRequest;
use crate::core::{consensus, util};
use crate::{lock_for_read, lock_for_write};

#[derive(Debug)]
pub enum RoutingEvent {
    BlockchainUpdated,
    StartBlockIdUpdated(BlockId),
}

#[derive(Debug)]
pub enum PeerState {
    Connected,
    Connecting,
    Disconnected,
}

pub struct StaticPeer {
    pub peer_details: util::configuration::PeerConfig,
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
    pub mempool: Arc<RwLock<Mempool>>,
    pub sender_to_consensus: Sender<ConsensusEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    // TODO : remove this if not needed
    pub static_peers: Vec<StaticPeer>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub network: Network,
    pub reconnection_timer: Timestamp,
    pub stats: RoutingStats,
    pub senders_to_verification: Vec<Sender<VerifyRequest>>,
    pub last_verification_thread_index: usize,
    pub stat_sender: Sender<String>,
    pub blockchain_sync_state: BlockchainSyncState,
    // TODO : can be removed since we are handling reconnection for each peer now
    pub initial_connection: bool,
    // TODO : can be removed since we are handling reconnection for each peer now
    pub reconnection_wait_time: Timestamp,
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
    async fn process_incoming_message(&mut self, peer_index: u64, message: Message) {
        trace!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            peer_index
        );
        self.network.update_peer_timer(peer_index).await;

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

            Message::Block(_) => {
                unreachable!("received block");
            }
            Message::Transaction(transaction) => {
                trace!(
                    "received transaction : {:?}",
                    transaction.signature.to_hex()
                );
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
            Message::Services(services) => {
                self.process_peer_services(services, peer_index).await;
            }
            Message::GhostChain(chain) => {
                self.process_ghost_chain(chain, peer_index).await;
            }
            Message::GhostChainRequest(block_id, block_hash, fork_id) => {
                self.process_ghost_chain_request(block_id, block_hash, fork_id, peer_index)
                    .await;
            }
            Message::ApplicationMessage(api_message) => {
                trace!(
                    "processing application msg with buffer size : {:?}",
                    api_message.data.len()
                );
                self.network
                    .io_interface
                    .process_api_call(api_message.data, api_message.msg_index, peer_index)
                    .await;
            }
            Message::Result(api_message) => {
                self.network
                    .io_interface
                    .process_api_success(api_message.data, api_message.msg_index, peer_index)
                    .await;
            }
            Message::Error(api_message) => {
                self.network
                    .io_interface
                    .process_api_error(api_message.data, api_message.msg_index, peer_index)
                    .await;
            }
        }
        trace!("incoming message processed");
    }
    async fn process_ghost_chain_request(
        &self,
        block_id: u64,
        block_hash: SaitoHash,
        fork_id: SaitoHash,
        peer_index: u64,
    ) {
        debug!("processing ghost chain request from peer : {:?}. block_id : {:?} block_hash: {:?} fork_id: {:?}",
            peer_index,
            block_id,
           block_hash.to_hex(),
            fork_id.to_hex()
        );
        let (blockchain, _blockchain_) = lock_for_read!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
        let peer_public_key;
        {
            let (peers, _peers_) = lock_for_read!(self.network.peers, LOCK_ORDER_PEERS);
            peer_public_key = peers
                .find_peer_by_index(peer_index)
                .unwrap()
                .public_key
                .unwrap();
        }

        let mut last_shared_ancestor = blockchain.generate_last_shared_ancestor(block_id, fork_id);

        debug!("last_shared_ancestor 1 : {:?}", last_shared_ancestor);

        if last_shared_ancestor == 0 && blockchain.get_latest_block_id() > 10 {
            last_shared_ancestor = blockchain.get_latest_block_id() - 10;
        }

        let start = blockchain
            .blockring
            .get_longest_chain_block_hash_at_block_id(last_shared_ancestor);
        let mut ghost = GhostChainSync {
            start,
            prehashes: vec![],
            previous_block_hashes: vec![],
            block_ids: vec![],
            block_ts: vec![],
            txs: vec![],
            gts: vec![],
        };
        let latest_block_id = blockchain.blockring.get_latest_block_id();
        debug!("latest_block_id : {:?}", latest_block_id);
        debug!("last_shared_ancestor : {:?}", last_shared_ancestor);
        debug!("start : {:?}", start.to_hex());
        for i in (last_shared_ancestor + 1)..=latest_block_id {
            let hash = blockchain
                .blockring
                .get_longest_chain_block_hash_at_block_id(i);
            if hash != [0; 32] {
                let block = blockchain.get_block(&hash);
                if let Some(block) = block {
                    debug!(
                        "pushing block : {:?} at index : {:?}",
                        block.hash.to_hex(),
                        i
                    );
                    ghost.gts.push(block.has_golden_ticket);
                    ghost.block_ts.push(block.timestamp);
                    ghost.prehashes.push(block.pre_hash);
                    ghost.previous_block_hashes.push(block.previous_block_hash);
                    ghost.block_ids.push(block.id);

                    // TODO : shouldn't this check for whole key list instead of peer's key?
                    ghost.txs.push(block.has_keylist_txs(vec![peer_public_key]));
                }
            }
        }

        debug!("sending ghost chain to peer : {:?}", peer_index);
        debug!("ghost : {:?}", ghost);
        let buffer = Message::GhostChain(ghost).serialize();
        self.network
            .io_interface
            .send_message(peer_index, buffer)
            .await
            .unwrap();
    }

    async fn handle_new_peer(
        &mut self,
        peer_data: Option<util::configuration::PeerConfig>,
        peer_index: u64,
    ) {
        trace!("handling new peer : {:?}", peer_index);
        self.network.handle_new_peer(peer_data, peer_index).await;
    }

    async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        self.network.handle_peer_disconnect(peer_index).await;
    }

    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        peer_index: u64,
    ) {
        info!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            request.latest_block_hash.to_hex(),
            request.fork_id.to_hex(),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        let (blockchain, _blockchain_) = lock_for_read!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);

        let last_shared_ancestor =
            blockchain.generate_last_shared_ancestor(request.latest_block_id, request.fork_id);
        debug!("last shared ancestor = {:?}", last_shared_ancestor);

        for i in last_shared_ancestor..(blockchain.blockring.get_latest_block_id() + 1) {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_at_block_id(i);
            if block_hash == [0; 32] {
                // TODO : can the block hash not be in the ring if we are going through the longest chain ?
                continue;
            }
            debug!("sending block header for : {:?}", block_hash.to_hex());
            let buffer = Message::BlockHeaderHash(block_hash, i).serialize();
            self.network
                .io_interface
                .send_message(peer_index, buffer)
                .await
                .unwrap();
        }
    }
    async fn process_incoming_block_hash(
        &mut self,
        block_hash: SaitoHash,
        block_id: u64,
        peer_index: u64,
    ) {
        debug!(
            "processing incoming block hash : {:?} from peer : {:?}",
            block_hash.to_hex(),
            peer_index
        );

        self.blockchain_sync_state
            .add_entry(block_hash, block_id, peer_index);

        self.fetch_next_blocks().await;
    }
    async fn fetch_next_blocks(&mut self) {
        self.blockchain_sync_state.build_peer_block_picture();

        let map = self.blockchain_sync_state.request_blocks_from_waitlist();

        let mut fetched_blocks: Vec<(PeerIndex, SaitoHash)> = Default::default();
        for (peer_index, vec) in map {
            for hash in vec.iter() {
                let result = self
                    .network
                    .process_incoming_block_hash(
                        *hash,
                        peer_index,
                        self.blockchain.clone(),
                        self.mempool.clone(),
                    )
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
        trace!("sending verification request to thread");
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
                trace!(
                    "verification request sent to verification thread : {:?}",
                    sender_index
                );
                return;
            }
            if trials == sender_count {
                // todo : if all the channels are full, we should wait here. cannot sleep to support wasm interface
                trials = 0;
            }
        }
    }
    async fn process_ghost_chain(&self, chain: GhostChainSync, peer_index: u64) {
        debug!("processing ghost chain from peer : {:?}", peer_index);
        debug!("ghost : {:?}", chain);

        let mut previous_block_hash = chain.start;
        let peer_key;
        {
            let (peers, _peers_) = lock_for_read!(self.network.peers, LOCK_ORDER_PEERS);
            peer_key = peers
                .find_peer_by_index(peer_index)
                .unwrap()
                .public_key
                .unwrap();
        }
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
        for i in 0..chain.prehashes.len() {
            let buf = [
                previous_block_hash.as_slice(),
                chain.prehashes[i].as_slice(),
            ]
            .concat();
            let block_hash = hash(&buf);
            if chain.txs[i] {
                debug!(
                    "ghost block : {:?} has txs for me. fetching",
                    block_hash.to_hex()
                );
                if !blockchain.is_block_fetching(&block_hash) {
                    blockchain.mark_as_fetching(block_hash);
                    let result = self
                        .network
                        .fetch_missing_block(block_hash, &peer_key)
                        .await;
                    if result.is_err() {
                        warn!("failed fetching block : {:?}. so unmarking block as fetching for ghost chain",block_hash.to_hex());
                        blockchain.unmark_as_fetching(&block_hash);
                    }
                }
            } else {
                debug!(
                    "ghost block : {:?} doesn't have txs for me. not fetching",
                    block_hash.to_hex()
                );
                blockchain.add_ghost_block(
                    chain.block_ids[i],
                    chain.previous_block_hashes[i],
                    chain.block_ts[i],
                    chain.prehashes[i],
                    chain.gts[i],
                    block_hash,
                );
            }
            previous_block_hash = block_hash;
        }
    }

    // TODO : remove if not required
    async fn process_peer_services(&mut self, services: Vec<PeerService>, peer_index: u64) {
        let (mut peers, _peers_) = lock_for_write!(self.network.peers, LOCK_ORDER_PEERS);
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_some() {
            let peer = peer.unwrap();
            peer.services = services;
        } else {
            warn!("peer {:?} not found to update services", peer_index);
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
                // trace!(
                //     "incoming message received from peer : {:?} buffer_len : {:?}",
                //     peer_index,
                //     buffer.len()
                // );
                let buffer_len = buffer.len();
                let message = Message::deserialize(buffer);
                if message.is_err() {
                    warn!(
                        "failed deserializing msg from peer : {:?} with buffer size : {:?}. disconnecting peer",
                        peer_index, buffer_len
                    );
                    self.network
                        .io_interface
                        .disconnect_from_peer(peer_index)
                        .await
                        .unwrap();
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
                debug!("block received : {:?}", block_hash.to_hex());

                self.send_to_verification_thread(VerifyRequest::Block(buffer, peer_index))
                    .await;

                self.blockchain_sync_state
                    .mark_as_fetched(peer_index, block_hash);

                self.fetch_next_blocks().await;

                return Some(());
            }
            NetworkEvent::BlockFetchFailed { block_hash } => {
                debug!("block fetch failed : {:?}", block_hash.to_hex());
                let (mut blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
                warn!("failed fetching block : {:?} from network thread. so unmarking block as fetching",block_hash.to_hex());

                blockchain.unmark_as_fetching(&block_hash);
            }
            NetworkEvent::DisconnectFromPeer { .. } => {
                todo!()
            }
        }
        debug!("network event processed");
        None
    }
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        let duration_value: Timestamp = duration.as_millis() as Timestamp;

        if !self.initial_connection {
            self.network.connect_to_static_peers().await;
            self.initial_connection = true;
        } else if self.initial_connection {
            self.reconnection_timer += duration_value;
            if self.reconnection_timer >= self.reconnection_wait_time {
                self.network.connect_to_static_peers().await;
                self.network.send_pings().await;
                self.reconnection_timer = 0;
            }
        }
        None
    }

    async fn process_event(&mut self, event: RoutingEvent) -> Option<()> {
        match event {
            RoutingEvent::BlockchainUpdated => {
                trace!("received blockchain update event");
                self.fetch_next_blocks().await;
            }
            RoutingEvent::StartBlockIdUpdated(block_id) => {
                trace!("start block id received as : {:?}", block_id);
                self.blockchain_sync_state
                    .set_latest_blockchain_id(block_id);
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
            "{} - {} - capacity : {:?} / {:?}",
            current_time,
            format!("{:width$}", "consensus::queue", width = 40),
            self.sender_to_consensus.capacity(),
            self.sender_to_consensus.max_capacity()
        );
        self.stat_sender.send(stat).await.unwrap();
        for (index, sender) in self.senders_to_verification.iter().enumerate() {
            let stat = format!(
                "{} - {} - capacity : {:?} / {:?}",
                current_time,
                format!(
                    "{:width$}",
                    format!("verification_{:?}::queue", index),
                    width = 40
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

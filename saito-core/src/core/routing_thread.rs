use ahash::HashMap;
use async_trait::async_trait;
use log::{debug, info, trace, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::blockchain_sync_state::BlockchainSyncState;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::peers::peer_service::PeerService;
use crate::core::consensus::peers::peer_state_writer::{PeerStateEntry, PEER_STATE_WRITE_PERIOD};
use crate::core::consensus::wallet::Wallet;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::defs::{
    BlockHash, BlockId, PeerIndex, PrintForLog, SaitoHash, SaitoPublicKey, StatVariable, Timestamp,
    STAT_BIN_COUNT,
};
use crate::core::io::network::{Network, PeerDisconnectType};
use crate::core::io::network_event::NetworkEvent;
use crate::core::mining_thread::MiningEvent;
use crate::core::msg::block_request::BlockchainRequest;
use crate::core::msg::ghost_chain_sync::GhostChainSync;
use crate::core::msg::message::Message;
use crate::core::process::keep_time::Timer;
use crate::core::process::process_event::ProcessEvent;
use crate::core::process::version::Version;
use crate::core::util;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::hash;
use crate::core::verification_thread::VerifyRequest;

#[derive(Debug)]
pub enum RoutingEvent {
    BlockchainUpdated(BlockHash),
    BlockFetchRequest(PeerIndex, BlockHash, BlockId),
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
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub mempool_lock: Arc<RwLock<Mempool>>,
    pub sender_to_consensus: Sender<ConsensusEvent>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub timer: Timer,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub network: Network,
    pub reconnection_timer: Timestamp,
    pub peer_removal_timer: Timestamp,
    pub peer_file_write_timer: Timestamp,
    pub stats: RoutingStats,
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
    async fn process_incoming_message(&mut self, peer_index: u64, message: Message) {
        self.network.update_peer_timer(peer_index).await;
        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                self.network
                    .handle_handshake_challenge(
                        peer_index,
                        challenge,
                        self.wallet_lock.clone(),
                        self.config_lock.clone(),
                    )
                    .await;
            }
            Message::HandshakeResponse(response) => {
                self.network
                    .handle_handshake_response(
                        peer_index,
                        response,
                        self.wallet_lock.clone(),
                        self.blockchain_lock.clone(),
                        self.config_lock.clone(),
                    )
                    .await;
            }

            Message::Transaction(transaction) => {
                self.stats.received_transactions.increment();
                self.send_to_verification_thread(VerifyRequest::Transaction(transaction))
                    .await;
            }
            Message::BlockchainRequest(request) => {
                self.process_incoming_blockchain_request(request, peer_index)
                    .await;
            }
            Message::BlockHeaderHash(hash, block_id) => {
                self.process_incoming_block_hash(hash, block_id, peer_index)
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
            Message::KeyListUpdate(key_list) => {
                self.network
                    .handle_received_key_list(peer_index, key_list)
                    .await;
            }
            _ => unreachable!(),
        }
    }
    /// Processes a received ghost chain request from a peer to sync itself with the blockchain
    ///
    /// # Arguments
    ///
    /// * `block_id`:
    /// * `block_hash`:
    /// * `fork_id`:
    /// * `peer_index`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
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
        let blockchain = self.blockchain_lock.read().await;
        let peer_public_key;
        {
            let peers = self.network.peer_lock.read().await;
            peer_public_key = peers
                .find_peer_by_index(peer_index)
                .unwrap()
                .get_public_key()
                .unwrap();
        }

        let ghost = Self::generate_ghost_chain(block_id, fork_id, blockchain, peer_public_key);

        debug!("sending ghost chain to peer : {:?}", peer_index);
        // debug!("ghost : {:?}", ghost);
        let buffer = Message::GhostChain(ghost).serialize();
        self.network
            .io_interface
            .send_message(peer_index, buffer.as_slice())
            .await
            .unwrap();
    }

    pub(crate) fn generate_ghost_chain(
        block_id: u64,
        fork_id: SaitoHash,
        blockchain: RwLockReadGuard<Blockchain>,
        peer_public_key: SaitoPublicKey,
    ) -> GhostChainSync {
        let mut last_shared_ancestor = blockchain.generate_last_shared_ancestor(block_id, fork_id);

        debug!("last_shared_ancestor 1 : {:?}", last_shared_ancestor);

        if last_shared_ancestor == 0 && blockchain.get_latest_block_id() > 10 {
            // if we cannot find the last shared ancestor in a long chain, we just need to sync the latest 10 blocks
            last_shared_ancestor = blockchain.get_latest_block_id() - 10;
        }

        let start = blockchain
            .blockring
            .get_longest_chain_block_hash_at_block_id(last_shared_ancestor);

        let latest_block_id = blockchain.blockring.get_latest_block_id();
        debug!("latest_block_id : {:?}", latest_block_id);
        debug!("last_shared_ancestor : {:?}", last_shared_ancestor);
        debug!("start : {:?}", start.to_hex());

        let mut ghost = GhostChainSync {
            start,
            prehashes: vec![],
            previous_block_hashes: vec![],
            block_ids: vec![],
            block_ts: vec![],
            txs: vec![],
            gts: vec![],
        };
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
                    // whether this block has any txs which the peer will be interested in
                    ghost.txs.push(block.has_keylist_txs(vec![peer_public_key]));
                }
            }
        }
        ghost
    }

    async fn handle_new_peer(&mut self, peer_index: u64) {
        trace!("handling new peer : {:?}", peer_index);
        self.network.handle_new_peer(peer_index).await;
    }

    async fn handle_new_stun_peer(&mut self, peer_index: u64, public_key: SaitoPublicKey ) {
        trace!("handling new stun peer : {:?}", peer_index);
        self.network.handle_new_stun_peer(peer_index, public_key ).await;
    }

    async fn remove_stun_peer(&mut self, peer_index: u64 ) {
        trace!("removing stun peer : {:?}", peer_index);
        self.network.remove_stun_peer(peer_index ).await;
    }


    async fn handle_peer_disconnect(
        &mut self,
        peer_index: u64,
        disconnect_type: PeerDisconnectType,
    ) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        self.network
            .handle_peer_disconnect(peer_index, disconnect_type)
            .await;
    }
    pub async fn set_my_key_list(&mut self, key_list: Vec<SaitoPublicKey>) {
        let mut wallet = self.wallet_lock.write().await;

        wallet.set_key_list(key_list);
        self.network.send_key_list(&wallet.key_list).await;
    }

    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        peer_index: u64,
    ) {
        debug!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            request.latest_block_hash.to_hex(),
            request.fork_id.to_hex(),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        let blockchain = self.blockchain_lock.read().await;

        let last_shared_ancestor =
            blockchain.generate_last_shared_ancestor(request.latest_block_id, request.fork_id);
        debug!(
            "last shared ancestor = {:?} latest_id = {:?}",
            last_shared_ancestor,
            blockchain.blockring.get_latest_block_id()
        );

        for i in last_shared_ancestor..(blockchain.blockring.get_latest_block_id() + 1) {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_at_block_id(i);
            if block_hash == [0; 32] {
                // TODO : can the block hash not be in the ring if we are going through the longest chain ?
                continue;
            }
            // trace!("sending block header for : {:?}", block_hash.to_hex());
            let buffer = Message::BlockHeaderHash(block_hash, i).serialize();
            self.network
                .io_interface
                .send_message(peer_index, buffer.as_slice())
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

        let peers = self.network.peer_lock.read().await;
        let wallet = self.wallet_lock.read().await;

        if let Some(peer) = peers.index_to_peers.get(&peer_index) {
            // TODO : check if this check can be removed from here, since network.rs also have the same check
            if wallet.wallet_version > peer.wallet_version
                && peer.wallet_version != Version::new(0, 0, 0)
            {
                warn!(
                    "Not Fetching Block: {:?} from peer :{:?} since peer version is old. expected: {:?} actual {:?} ",
                    block_hash.to_hex(), peer.index, wallet.wallet_version, peer.wallet_version
                );
                return;
            }
        }

        drop(peers);
        drop(wallet);

        self.blockchain_sync_state
            .add_entry(
                block_hash,
                block_id,
                peer_index,
                self.network.peer_lock.clone(),
            )
            .await;

        self.fetch_next_blocks().await;
    }

    async fn fetch_next_blocks(&mut self) -> bool {
        // trace!("fetching next blocks from peers");
        let mut work_done = false;
        {
            let blockchain = self.blockchain_lock.read().await;
            self.blockchain_sync_state
                .build_peer_block_picture(&blockchain);
        }

        let map = self.blockchain_sync_state.get_blocks_to_fetch_per_peer();

        let mut fetched_blocks: Vec<(PeerIndex, SaitoHash)> = Default::default();
        for (peer_index, vec) in map {
            for (hash, block_id) in vec.iter() {
                work_done = true;
                let result = self
                    .network
                    .process_incoming_block_hash(
                        *hash,
                        *block_id,
                        peer_index,
                        self.blockchain_lock.clone(),
                        self.mempool_lock.clone(),
                    )
                    .await;
                if result.is_some() {
                    fetched_blocks.push((peer_index, *hash));
                } else {
                    // if we already have the block added don't need to request it from peer
                    self.blockchain_sync_state.remove_entry(*hash);
                }
            }
        }
        work_done
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
                // todo : if all the channels are full, we should wait here. cannot sleep to support wasm interface
                trials = 0;
            }
        }
    }
    async fn process_ghost_chain(&mut self, chain: GhostChainSync, peer_index: u64) {
        debug!("processing ghost chain from peer : {:?}", peer_index);

        let mut previous_block_hash = chain.start;
        let mut blockchain = self.blockchain_lock.write().await;
        for i in 0..chain.prehashes.len() {
            let buf = [
                previous_block_hash.as_slice(),
                chain.prehashes[i].as_slice(),
            ]
            .concat();
            let block_hash = hash(&buf);
            if chain.txs[i] {
                debug!(
                    "ghost block : {:?} has txs for me. fetching from peer : {:?}",
                    block_hash.to_hex(),
                    peer_index
                );
                self.blockchain_sync_state
                    .add_entry(
                        block_hash,
                        chain.block_ids[i],
                        peer_index,
                        self.network.peer_lock.clone(),
                    )
                    .await;
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
        let mut peers = self.network.peer_lock.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_some() {
            let peer = peer.unwrap();
            peer.services = services;
        } else {
            warn!("peer {:?} not found to update services", peer_index);
        }
    }

    async fn write_peer_state_data(&mut self, duration_value: Timestamp, work_done: &mut bool) {
        self.peer_file_write_timer += duration_value;
        if self.peer_file_write_timer >= PEER_STATE_WRITE_PERIOD {
            let mut peers = self.network.peer_lock.write().await;
            let mut data: Vec<PeerStateEntry> = Default::default();

            let current_time = self.timer.get_timestamp_in_ms();

            for (peer_index, peer) in peers.index_to_peers.iter_mut() {
                if peer.public_key.is_none() {
                    info!("public key not set yet for peer : {:?}", peer_index);
                    continue;
                }
                data.push(PeerStateEntry {
                    peer_index: peer.index,
                    public_key: peer.public_key.unwrap_or([0; 33]),
                    msg_limit_exceeded: peer.has_message_limit_exceeded(current_time),
                    invalid_blocks_received: peer.has_invalid_block_limit_exceeded(current_time),
                    same_depth_blocks_received: false,
                    too_far_blocks_received: false,
                    handshake_limit_exceeded: peer.has_handshake_limit_exceeded(current_time),
                    keylist_limit_exceeded: peer.has_key_list_limit_exceeded(current_time),
                    limited_till: None,
                });
            }
            peers
                .peer_state_writer
                .write_state(data, &mut self.network.io_interface)
                .await
                .unwrap();
            self.peer_file_write_timer = 0;
            *work_done = true;
        }
    }
}

#[async_trait]
impl ProcessEvent<RoutingEvent> for RoutingThread {
    async fn process_network_event(&mut self, event: NetworkEvent) -> Option<()> {
      
        match event {
            NetworkEvent::IncomingNetworkMessage { peer_index, buffer } => {
                {
                    let mut peers = self.network.peer_lock.write().await;
                    let peer = peers.find_peer_by_index_mut(peer_index)?;

                
                    let time: u64 = self.timer.get_timestamp_in_ms();
                    peer.message_limiter.increase();
                    if peer.has_message_limit_exceeded(time) {
                        info!("peers exceeded for messages from peer : {:?}", peer_index);
                        return None;
                    }
                }
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
            NetworkEvent::PeerConnectionResult { result } => {
                if result.is_ok() {
                    self.handle_new_peer(result.unwrap()).await;
                    return Some(());
                }
            }

            NetworkEvent::AddStunPeer { result } => {
                if result.is_ok() {
                    let (peer_index, public_key) = result.unwrap();
                    self.handle_new_stun_peer(peer_index, public_key).await;
                    return Some(());
                }
            }
            NetworkEvent::RemoveStunPeer { result } => {
                if result.is_ok() {
                    let peer_index= result.unwrap();
                    self.remove_stun_peer(peer_index).await;
                    return Some(());
                }
            }
            NetworkEvent::PeerDisconnected {
                peer_index,
                disconnect_type,
            } => {
                self.handle_peer_disconnect(peer_index, disconnect_type)
                    .await;
                return Some(());
            }
            NetworkEvent::BlockFetched {
                block_hash,
                block_id,
                peer_index,
                buffer,
            } => {
                debug!("block received : {:?}", block_hash.to_hex());
                {
                    let mut peers = self.network.peer_lock.write().await;
                    let peer = peers.find_peer_by_index_mut(peer_index)?;
                    let time = self.timer.get_timestamp_in_ms();
                    if peer.has_invalid_block_limit_exceeded(time) {
                        info!(
                            "peers exceeded for invalid blocks from peer : {:?}. disconnecting peer...",
                            peer_index
                        );
                        self.network
                            .io_interface
                            .disconnect_from_peer(peer_index)
                            .await
                            .unwrap();
                        return None;
                    }
                }

                self.send_to_verification_thread(VerifyRequest::Block(
                    buffer, peer_index, block_hash, block_id,
                ))
                .await;

                self.blockchain_sync_state.mark_as_fetched(block_hash);

                self.fetch_next_blocks().await;

                return Some(());
            }
            NetworkEvent::BlockFetchFailed {
                block_hash,
                peer_index,
                block_id,
            } => {
                debug!("block fetch failed : {:?}", block_hash.to_hex());

                self.blockchain_sync_state
                    .mark_as_failed(block_id, block_hash, peer_index);
            }
            _ => unreachable!(),
        }
        debug!("network event processed");
        None
    }
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        let duration_value: Timestamp = duration.as_millis() as Timestamp;

        let mut work_done = false;

        const RECONNECTION_PERIOD: Timestamp = 10_000;
        self.reconnection_timer += duration_value;
        let current_time = self.timer.get_timestamp_in_ms();
        if self.reconnection_timer >= RECONNECTION_PERIOD {
            self.network.connect_to_static_peers(current_time).await;
            self.network.send_pings().await;
            self.reconnection_timer = 0;
            self.fetch_next_blocks().await;
            work_done = true;
        }

        const PEER_REMOVAL_PERIOD: Timestamp = Duration::from_secs(60).as_millis() as Timestamp;
        self.peer_removal_timer += duration_value;
        if self.peer_removal_timer >= PEER_REMOVAL_PERIOD {
            let mut peers = self.network.peer_lock.write().await;
            peers.remove_disconnected_peers(current_time);
            self.peer_removal_timer = 0;
            work_done = true;
        }

        self.write_peer_state_data(duration_value, &mut work_done)
            .await;

        if work_done {
            return Some(());
        }
        None
    }

    async fn process_event(&mut self, event: RoutingEvent) -> Option<()> {
        match event {
            RoutingEvent::BlockchainUpdated(block_hash) => {
                trace!(
                    "received blockchain update event : {:?}",
                    block_hash.to_hex()
                );
                self.blockchain_sync_state.remove_entry(block_hash);
                self.fetch_next_blocks().await;
            }

            RoutingEvent::BlockFetchRequest(peer_index, block_hash, block_id) => {
                self.blockchain_sync_state
                    .add_entry(
                        block_hash,
                        block_id,
                        peer_index,
                        self.network.peer_lock.clone(),
                    )
                    .await;
            }
        }
        None
    }

    async fn on_init(&mut self) {
        assert!(!self.senders_to_verification.is_empty());
        self.reconnection_timer = self.timer.get_timestamp_in_ms();
        // connect to peers
        self.network
            .initialize_static_peers(self.config_lock.clone())
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
            StatVariable::format_timestamp(current_time),
            format!("{:width$}", "consensus::channel", width = 40),
            self.sender_to_consensus.capacity(),
            self.sender_to_consensus.max_capacity()
        );
        self.stat_sender.send(stat).await.unwrap();
        for (index, sender) in self.senders_to_verification.iter().enumerate() {
            let stat = format!(
                "{} - {} - capacity : {:?} / {:?}",
                StatVariable::format_timestamp(current_time),
                format!(
                    "{:width$}",
                    format!("verification_{:?}::channel", index),
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

        let peers = self.network.peer_lock.read().await;
        let mut peer_count = 0;
        let mut peers_in_handshake = 0;

        for (_, peer) in peers.index_to_peers.iter() {
            peer_count += 1;

            if peer.challenge_for_peer.is_some() {
                peers_in_handshake += 1;
            }
        }

        let stat = format!(
            "{} - {} - total peers : {:?}. in handshake : {:?}",
            StatVariable::format_timestamp(current_time),
            format!("{:width$}", "peers::state", width = 40),
            peer_count,
            peers_in_handshake,
        );
        self.stat_sender.send(stat).await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use crate::core::defs::NOLAN_PER_SAITO;
    use crate::core::routing_thread::RoutingThread;
    use crate::core::util::crypto::generate_keys;
    use crate::core::util::test::node_tester::test::NodeTester;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_ghost_chain_gen() {
        // pretty_env_logger::init();
        let peer_public_key = generate_keys().0;
        let mut tester = NodeTester::default();
        tester
            .init_with_staking(0, 60, 100_000 * NOLAN_PER_SAITO)
            .await
            .unwrap();

        tester.wait_till_block_id_with_txs(100, 0, 0).await.unwrap();

        {
            let fork_id = tester.get_fork_id(50).await;
            let blockchain = tester.routing_thread.blockchain_lock.read().await;

            let ghost_chain =
                RoutingThread::generate_ghost_chain(50, fork_id, blockchain, peer_public_key);

            assert_eq!(ghost_chain.block_ids.len(), 10);
            assert_eq!(ghost_chain.block_ts.len(), 10);
            assert_eq!(ghost_chain.gts.len(), 10);
            assert_eq!(ghost_chain.prehashes.len(), 10);
            assert_eq!(ghost_chain.previous_block_hashes.len(), 10);
            assert!(ghost_chain.txs.iter().all(|x| !(*x)));
        }

        {
            let tx = tester
                .create_transaction(100, 10, peer_public_key)
                .await
                .unwrap();
            tester.add_transaction(tx).await;
        }

        tester.wait_till_block_id(101).await.unwrap();

        tester
            .wait_till_block_id_with_txs(105, 10, 0)
            .await
            .unwrap();

        {
            let block_id = 101;
            let fork_id = tester.get_fork_id(block_id).await;
            let blockchain = tester.routing_thread.blockchain_lock.read().await;
            let ghost_chain =
                RoutingThread::generate_ghost_chain(block_id, fork_id, blockchain, peer_public_key);

            assert_eq!(ghost_chain.block_ids.len(), 5);
            assert_eq!(ghost_chain.block_ts.len(), 5);
            assert_eq!(ghost_chain.gts.len(), 5);
            assert_eq!(ghost_chain.prehashes.len(), 5);
            assert_eq!(ghost_chain.previous_block_hashes.len(), 5);
            assert_eq!(ghost_chain.txs.iter().filter(|x| **x).count(), 1);
        }
    }
}

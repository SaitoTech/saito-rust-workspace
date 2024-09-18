use std::sync::Arc;

use log::{debug, error, info, trace, warn};
use tokio::sync::RwLock;

use crate::core::consensus::block::Block;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::peer::{Peer, PeerStatus};
use crate::core::consensus::peer_collection::PeerCollection;
use crate::core::consensus::transaction::{Transaction, TransactionType};
use crate::core::consensus::wallet::Wallet;
use crate::core::defs::{BlockId, PeerIndex, PrintForLog, SaitoHash, SaitoPublicKey, Timestamp};
use crate::core::io::interface_io::{InterfaceEvent, InterfaceIO};
use crate::core::msg::block_request::BlockchainRequest;
use crate::core::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::msg::message::Message;
use crate::core::process::keep_time::Timer;
use crate::core::util::configuration::Configuration;

#[derive(Debug)]
pub enum PeerDisconnectType {
    /// If the peer was disconnected without our intervention
    ExternalDisconnect,
    /// If we disconnected the peer
    InternalDisconnect,
}

// #[derive(Debug)]
pub struct Network {
    // TODO : manage peers from network
    pub peer_lock: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub timer: Timer,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peer_lock: Arc<RwLock<PeerCollection>>,
        wallet_lock: Arc<RwLock<Wallet>>,
        config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
        timer: Timer,
    ) -> Network {
        Network {
            peer_lock,
            io_interface: io_handler,
            wallet_lock,
            config_lock,
            timer,
        }
    }
    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", block.hash.to_hex());

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node
        if let Some(index) = block.routed_from_peer.as_ref() {
            excluded_peers.push(*index);
        }

        {
            let peers = self.peer_lock.read().await;
            for (index, peer) in peers.index_to_peers.iter() {
                if peer.get_public_key().is_none() {
                    excluded_peers.push(*index);
                    continue;
                }
            }
        }

        debug!("sending block : {:?} to peers", block.hash.to_hex());
        let message = Message::BlockHeaderHash(block.hash, block.id);
        self.io_interface
            .send_message_to_all(message.serialize().as_slice(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, transaction: &Transaction) {
        // TODO : return if tx is not valid

        let peers = self.peer_lock.read().await;
        let mut wallet = self.wallet_lock.write().await;

        let public_key = wallet.public_key;

        if transaction
            .from
            .first()
            .expect("from slip should exist")
            .public_key
            == public_key
        {
            if let TransactionType::GoldenTicket = transaction.transaction_type {
            } else {
                wallet.add_to_pending(transaction.clone());
            }
        }

        for (index, peer) in peers.index_to_peers.iter() {
            if peer.get_public_key().is_none() {
                continue;
            }
            let public_key = peer.get_public_key().unwrap();
            if transaction.is_in_path(&public_key) {
                continue;
            }
            let mut transaction = transaction.clone();
            transaction.add_hop(&wallet.private_key, &wallet.public_key, &public_key);
            let message = Message::Transaction(transaction);
            self.io_interface
                .send_message(*index, message.serialize().as_slice())
                .await
                .unwrap();
        }
    }

    pub async fn handle_peer_disconnect(
        &mut self,
        peer_index: u64,
        disconnect_type: PeerDisconnectType,
    ) {
        debug!("handling peer disconnect, peer_index = {}", peer_index);

        if let PeerDisconnectType::ExternalDisconnect = disconnect_type {
            self.io_interface
                .disconnect_from_peer(peer_index)
                .await
                .unwrap();
        }

        let mut remove_peer = false;
        let mut peers = self.peer_lock.write().await;
        if let Some(peer) = peers.find_peer_by_index_mut(peer_index) {
            if peer.get_public_key().is_some() {
                // calling here before removing the peer from collections
                self.io_interface
                    .send_interface_event(InterfaceEvent::PeerConnectionDropped(
                        peer_index,
                        peer.get_public_key().unwrap(),
                    ));
            }
            if peer.static_peer_config.is_none() {
                // we remove the peer only if it's connected "from" outside
                remove_peer = true;
            }
            peer.mark_as_disconnected();
        } else {
            error!("unknown peer : {:?} disconnected", peer_index);
        }
        if remove_peer {
            debug!("removing peer : {:?} from peer collection", peer_index);
            peers.remove_peer(peer_index);
        }
    }
    pub async fn handle_new_peer(&mut self, peer_index: u64) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        let mut peers = self.peer_lock.write().await;

        if let Some(peer) = peers.find_peer_by_index_mut(peer_index) {
            debug!("static peer : {:?} connected", peer_index);
            peer.peer_status = PeerStatus::Connecting;
        } else {
            debug!("new peer added : {:?}", peer_index);
            let mut peer = Peer::new(peer_index);
            peer.peer_status = PeerStatus::Connecting;
            peers.index_to_peers.insert(peer_index, peer);
        }

        if let Some(peer) = peers.find_peer_by_index_mut(peer_index) {
            if peer.static_peer_config.is_none() {
                peer.initiate_handshake(self.io_interface.as_ref())
                    .await
                    .unwrap();
            }
        }

        debug!("current peer count = {:?}", peers.index_to_peers.len());
    }

    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet_lock: Arc<RwLock<Wallet>>,
        config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle handshake challenge",
                peer_index
            );
            return;
        }
        let peer = peer.unwrap();
        peer.handle_handshake_challenge(
            challenge,
            self.io_interface.as_ref(),
            wallet_lock.clone(),
            config_lock,
        )
        .await
        .unwrap();
    }
    pub async fn handle_handshake_response(
        &mut self,
        peer_index: u64,
        response: HandshakeResponse,
        wallet_lock: Arc<RwLock<Wallet>>,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        // debug!("handling handshake response");
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle handshake response",
                peer_index
            );
            return;
        }
        let peer = peer.unwrap();
        let result = peer
            .handle_handshake_response(
                response,
                self.io_interface.as_ref(),
                wallet_lock.clone(),
                configs_lock.clone(),
            )
            .await;
        if result.is_err() || peer.get_public_key().is_none() {
            info!(
                "disconnecting peer : {:?} as handshake response was not handled",
                peer_index
            );
            self.io_interface
                .disconnect_from_peer(peer_index)
                .await
                .unwrap();
            return;
        }
        let public_key = peer.get_public_key().unwrap();
        debug!(
            "peer : {:?} handshake successful for peer : {:?}",
            peer.index,
            public_key.to_base58()
        );
        self.io_interface
            .send_interface_event(InterfaceEvent::PeerConnected(peer_index));
        peers.address_to_peers.insert(public_key, peer_index);
        // start block syncing here
        self.request_blockchain_from_peer(peer_index, blockchain_lock.clone())
            .await;
    }
    pub async fn handle_received_key_list(
        &mut self,
        peer_index: PeerIndex,
        key_list: Vec<SaitoPublicKey>,
    ) {
        debug!(
            "handling received key list of length : {:?} from peer : {:?}",
            key_list.len(),
            peer_index
        );
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle received key list",
                peer_index
            );
            return;
        }
        let peer = peer.unwrap();

        peer.key_list = key_list;
    }

    pub async fn send_key_list(&self, key_list: &[SaitoPublicKey]) {
        debug!(
            "sending key list to all the peers {:?}",
            key_list
                .iter()
                .map(|key| key.to_base58())
                .collect::<Vec<String>>()
        );

        self.io_interface
            .send_message_to_all(
                Message::KeyListUpdate(key_list.to_vec())
                    .serialize()
                    .as_slice(),
                vec![],
            )
            .await
            .unwrap();
    }

    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        blockchain_lock: Arc<RwLock<Blockchain>>,
    ) {
        debug!("requesting blockchain from peer : {:?}", peer_index);

        let configs = self.config_lock.read().await;
        let blockchain = blockchain_lock.read().await;
        let fork_id = *blockchain.get_fork_id();
        let buffer: Vec<u8>;

        if configs.is_spv_mode() {
            let request;
            {
                if blockchain.last_block_id > blockchain.get_latest_block_id() {
                    debug!(
                        "blockchain request 1 : latest_id: {:?} latest_hash: {:?} fork_id: {:?}",
                        blockchain.last_block_id,
                        blockchain.last_block_hash.to_hex(),
                        fork_id.to_hex()
                    );
                    request = BlockchainRequest {
                        latest_block_id: blockchain.last_block_id,
                        latest_block_hash: blockchain.last_block_hash,
                        fork_id,
                    };
                } else {
                    debug!(
                        "blockchain request 2 : latest_id: {:?} latest_hash: {:?} fork_id: {:?}",
                        blockchain.get_latest_block_id(),
                        blockchain.get_latest_block_hash().to_hex(),
                        fork_id.to_hex()
                    );
                    request = BlockchainRequest {
                        latest_block_id: blockchain.get_latest_block_id(),
                        latest_block_hash: blockchain.get_latest_block_hash(),
                        fork_id,
                    };
                }
            }
            debug!("sending ghost chain request to peer : {:?}", peer_index);
            buffer = Message::GhostChainRequest(
                request.latest_block_id,
                request.latest_block_hash,
                request.fork_id,
            )
            .serialize();
        } else {
            let request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id,
            };
            debug!("sending blockchain request to peer : {:?}", peer_index);
            buffer = Message::BlockchainRequest(request).serialize();
        }
        // need to drop the reference here to avoid deadlocks.
        // We need blockchain lock till here to avoid integrity issues
        drop(blockchain);
        drop(configs);

        self.io_interface
            .send_message(peer_index, buffer.as_slice())
            .await
            .unwrap();
    }
    pub async fn process_incoming_block_hash(
        &mut self,
        block_hash: SaitoHash,
        block_id: BlockId,
        peer_index: PeerIndex,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        mempool_lock: Arc<RwLock<Mempool>>,
    ) -> Option<()> {
        trace!(
            "processing incoming block hash : {:?} from peer : {:?}",
            block_hash.to_hex(),
            peer_index
        );
        let configs = self.config_lock.read().await;
        let block_exists;
        let my_public_key;
        {
            let blockchain = blockchain_lock.read().await;
            let mempool = mempool_lock.read().await;
            let wallet = self.wallet_lock.read().await;

            block_exists = blockchain.is_block_indexed(block_hash)
                || mempool.blocks_queue.iter().any(|b| b.hash == block_hash);
            my_public_key = wallet.public_key;
        }
        if block_exists {
            debug!(
                "block : {:?}-{:?} already exists in chain. not fetching",
                block_id,
                block_hash.to_hex()
            );
            return None;
        }
        let url;
        {
            let peers = self.peer_lock.read().await;
            let wallet = self.wallet_lock.read().await;

            if let Some(peer) = peers.index_to_peers.get(&peer_index) {
                if wallet.wallet_version > peer.wallet_version {
                    warn!(
                    "Not Fetching Block: {:?} from peer :{:?} since peer version is old. expected: {:?} actual {:?} ",
                    block_hash.to_hex(), peer.index, wallet.wallet_version, peer.wallet_version
                );
                    return None;
                }

                if peer.block_fetch_url.is_empty() {
                    debug!(
                        "won't fetch block : {:?} from peer : {:?} since no url found",
                        block_hash.to_hex(),
                        peer_index
                    );
                    return None;
                }
                url = peer.get_block_fetch_url(block_hash, configs.is_spv_mode(), my_public_key);
            } else {
                warn!(
                    "peer : {:?} is not in peer list. cannot generate the block fetch url",
                    peer_index
                );
                return None;
            }
        }

        debug!(
            "fetching block for incoming hash : {:?}-{:?}",
            block_id,
            block_hash.to_hex()
        );

        if self
            .io_interface
            .fetch_block_from_peer(block_hash, peer_index, url.as_str(), block_id)
            .await
            .is_err()
        {
            // failed fetching block from peer
            warn!(
                "failed fetching block : {:?} for block hash. so unmarking block as fetching",
                block_hash.to_hex()
            );
        }
        Some(())
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let configs = configs_lock.read().await;
        let mut peers = self.peer_lock.write().await;

        // TODO : can create a new disconnected peer with a is_static flag set. so we don't need to keep the static peers separately
        configs
            .get_peer_configs()
            .clone()
            .drain(..)
            .for_each(|config| {
                let mut peer = Peer::new(peers.peer_counter.get_next_index());

                peer.static_peer_config = Some(config);

                peers.index_to_peers.insert(peer.index, peer);
            });

        info!("added {:?} static peers", peers.index_to_peers.len());
    }

    pub async fn connect_to_static_peers(&mut self, current_time: Timestamp) {
        trace!("connecting to static peers...");
        let mut peers = self.peer_lock.write().await;
        for (peer_index, peer) in &mut peers.index_to_peers {
            let url = peer.get_url();
            if let PeerStatus::Disconnected(connect_time, period) = &mut peer.peer_status {
                if current_time < *connect_time {
                    continue;
                }
                debug!("static peer : {:?} is disconnected", peer_index);
                if let Some(config) = peer.static_peer_config.as_ref() {
                    debug!(
                        "trying to connect to static peer : {:?} with {:?}",
                        peer_index, config
                    );
                    self.io_interface
                        .connect_to_peer(url, peer.index)
                        .await
                        .unwrap();
                    *period *= 2;
                    *connect_time = current_time + *period;
                } else {
                    error!("static peer : {:?} doesn't have configs set", peer_index);
                }
            }
        }
    }

    pub async fn send_pings(&mut self) {
        let current_time = self.timer.get_timestamp_in_ms();
        let mut peers = self.peer_lock.write().await;
        for (_, peer) in peers.index_to_peers.iter_mut() {
            if peer.get_public_key().is_some() {
                peer.send_ping(current_time, self.io_interface.as_ref())
                    .await;
            }
        }
    }

    pub async fn update_peer_timer(&mut self, peer_index: PeerIndex) {
        let mut peers = self.peer_lock.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            return;
        }
        let peer = peer.unwrap();
        peer.last_msg_at = self.timer.get_timestamp_in_ms();
    }
}

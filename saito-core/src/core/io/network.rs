use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::{debug, error, info, trace, warn};
use tokio::sync::RwLock;

use crate::common::defs::{
    push_lock, PeerIndex, PrintForLog, SaitoHash, SaitoPublicKey, Timestamp, LOCK_ORDER_BLOCKCHAIN,
    LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_PEERS, LOCK_ORDER_WALLET,
    PEER_RECONNECT_WAIT_PERIOD,
};
use crate::common::keep_time::KeepTime;
use crate::core::consensus::block::Block;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::peer::Peer;
use crate::core::consensus::peer_collection::PeerCollection;
use crate::core::consensus::transaction::{Transaction, TransactionType};
use crate::core::consensus::wallet::Wallet;
use crate::core::io::interface_io::{InterfaceEvent, InterfaceIO};
use crate::core::msg::block_request::BlockchainRequest;
use crate::core::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::msg::message::Message;
use crate::core::util::configuration::{Configuration, PeerConfig};
use crate::{lock_for_read, lock_for_write};

// #[derive(Debug)]
pub struct Network {
    // TODO : manage peers from network
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    static_peer_configs: Vec<(PeerConfig, Timestamp)>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
        time_keeper: Box<dyn KeepTime + Send + Sync>,
    ) -> Network {
        Network {
            peers,
            io_interface: io_handler,
            static_peer_configs: Default::default(),
            wallet,
            configs,
            time_keeper,
        }
    }
    pub async fn propagate_block(
        &self,
        block: &Block,
        configs: &(dyn Configuration + Send + Sync),
    ) {
        debug!("propagating block : {:?}", block.hash.to_hex());
        if configs.is_browser() {
            trace!("not propagating block since we are in browser");
            return;
        }

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node

        {
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);
            for (index, peer) in peers.index_to_peers.iter() {
                if peer.public_key.is_none() {
                    excluded_peers.push(*index);
                    continue;
                }
                if block.source_connection_id.is_some() {
                    let peer = peers
                        .address_to_peers
                        .get(&block.source_connection_id.unwrap());
                    if peer.is_some() {
                        excluded_peers.push(*peer.unwrap());
                        continue;
                    }
                }
            }
        }

        debug!("sending block : {:?} to peers", block.hash.to_hex());
        let message = Message::BlockHeaderHash(block.hash, block.id);
        self.io_interface
            .send_message_to_all(message.serialize(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, transaction: &Transaction) {
        // TODO : return if tx is not valid

        let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);
        let (mut wallet, _wallet_) = lock_for_write!(self.wallet, LOCK_ORDER_WALLET);

        trace!(
            "propagating transaction : {:?} peers : {:?}",
            transaction.signature.to_hex(),
            peers.index_to_peers.len()
        );

        let public_key = wallet.public_key;

        if transaction
            .from
            .get(0)
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
            if peer.public_key.is_none() {
                continue;
            }
            if transaction.is_in_path(peer.public_key.as_ref().unwrap()) {
                continue;
            }
            let mut transaction = transaction.clone();
            transaction.add_hop(
                &wallet.private_key,
                &wallet.public_key,
                peer.public_key.as_ref().unwrap(),
            );
            let message = Message::Transaction(transaction);
            self.io_interface
                .send_message(*index, message.serialize())
                .await
                .unwrap();
        }
    }

    pub async fn fetch_missing_block(
        &self,
        block_hash: SaitoHash,
        public_key: &SaitoPublicKey,
    ) -> Result<(), Error> {
        debug!(
            "fetch missing block : block : {:?} from : {:?}",
            block_hash.to_hex(),
            public_key.to_base58()
        );

        let peer_index;
        let url;
        let my_public_key;
        {
            let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
                my_public_key = wallet.public_key;
            }

            let peer = peers.find_peer_by_address(public_key);
            if peer.is_none() {
                debug!("peer count = {:?}", peers.address_to_peers.len());
                error!(
                    "peer : {:?} not found to fetch missing block : {:?}",
                    public_key.to_base58(),
                    block_hash.to_hex()
                );
                return Err(Error::from(ErrorKind::NotFound));
            }
            let peer = peer.unwrap();
            if peer.block_fetch_url.is_empty() {
                warn!(
                    "won't fetch block : {:?} from peer : {:?} since no url found",
                    block_hash.to_hex(),
                    peer.index
                );
                return Err(Error::from(ErrorKind::AddrNotAvailable));
            }
            url = peer.get_block_fetch_url(block_hash, configs.is_spv_mode(), my_public_key);
            peer_index = peer.index;
        }
        self.io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
    }
    pub async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);

        self.io_interface
            .disconnect_from_peer(peer_index)
            .await
            .unwrap();

        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

        let result = peers.find_peer_by_index(peer_index);

        if result.is_some() {
            let peer = result.unwrap();

            if peer.public_key.is_some() {
                // calling here before removing the peer from collections
                self.io_interface
                    .send_interface_event(InterfaceEvent::PeerConnectionDropped(peer_index));
            }

            let public_key = peer.public_key;
            if peer.static_peer_config.is_some() {
                // This means the connection has been initiated from this side, therefore we must
                // try to re-establish the connection again
                // TODO : Add a delay so that there won't be a runaway issue with connects and
                // disconnects, check the best place to add (here or network_controller)
                info!(
                    "Static peer disconnected, reconnecting .., Peer ID = {}",
                    peer.index
                );

                // self.io_interface
                //     .connect_to_peer(peer.static_peer_config.as_ref().unwrap().clone())
                //     .await
                //     .unwrap();

                // setting to immediately reconnect. if failed, it will connect after a time
                self.static_peer_configs
                    .push((peer.static_peer_config.as_ref().unwrap().clone(), 0));
            } else if peer.public_key.is_some() {
                info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                peer.index, peer.public_key.as_ref().unwrap().to_base58());
            }

            if public_key.is_some() {
                peers.address_to_peers.remove(&public_key.unwrap());
            }
            peers.index_to_peers.remove(&peer_index);
        } else {
            error!("unknown peer : {:?} disconnected", peer_index);
        }
    }
    pub async fn handle_new_peer(&mut self, peer_data: Option<PeerConfig>, peer_index: u64) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

        let mut peer = Peer::new(peer_index);
        peer.static_peer_config = peer_data;

        if peer.static_peer_config.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(&self.io_interface).await.unwrap();
        } else {
            debug!(
                "removing static peer config : {:?}",
                peer.static_peer_config.as_ref().unwrap()
            );
            let data = peer.static_peer_config.as_ref().unwrap();

            self.static_peer_configs
                .retain(|(config, _reconnect_time)| {
                    config.host != data.host || config.port != data.port
                });
        }

        debug!("new peer added : {:?}", peer_index);
        peers.index_to_peers.insert(peer_index, peer);
        debug!("current peer count = {:?}", peers.index_to_peers.len());
    }
    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle handshake challenge",
                peer_index
            );
            return;
        }
        let peer = peer.unwrap();
        peer.handle_handshake_challenge(challenge, &self.io_interface, wallet.clone(), configs)
            .await
            .unwrap();
    }
    pub async fn handle_handshake_response(
        &mut self,
        peer_index: u64,
        response: HandshakeResponse,
        wallet: Arc<RwLock<Wallet>>,
        blockchain: Arc<RwLock<Blockchain>>,
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        debug!("handling handshake response");
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

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
                &self.io_interface,
                wallet.clone(),
                configs.clone(),
            )
            .await;
        if result.is_err() || peer.public_key.is_none() {
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
        debug!(
            "peer : {:?} handshake successful for peer : {:?}",
            peer.index,
            peer.public_key.as_ref().unwrap().to_base58()
        );
        self.io_interface
            .send_interface_event(InterfaceEvent::PeerConnected(peer_index));
        let public_key = peer.public_key.unwrap();
        peers.address_to_peers.insert(public_key, peer_index);
        // start block syncing here
        self.request_blockchain_from_peer(peer_index, blockchain.clone())
            .await;
    }

    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        blockchain_lock: Arc<RwLock<Blockchain>>,
    ) {
        info!("requesting blockchain from peer : {:?}", peer_index);

        // TODO : should this be moved inside peer ?
        let fork_id;
        {
            // TODO : will this create a race condition if we release the lock after reading fork id ?
            let (blockchain, _blockchain_) = lock_for_read!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            fork_id = *blockchain.get_fork_id();
        }

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        if configs.is_spv_mode() {
            let request;
            {
                let (blockchain, _blockchain_) =
                    lock_for_read!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

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
            let buffer = Message::GhostChainRequest(
                request.latest_block_id,
                request.latest_block_hash,
                request.fork_id,
            )
            .serialize();
            self.io_interface
                .send_message(peer_index, buffer)
                .await
                .unwrap();
        } else {
            let request;
            {
                let (blockchain, _blockchain_) =
                    lock_for_read!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

                request = BlockchainRequest {
                    latest_block_id: blockchain.get_latest_block_id(),
                    latest_block_hash: blockchain.get_latest_block_hash(),
                    fork_id,
                };
            }
            debug!("sending blockchain request to peer : {:?}", peer_index);
            let buffer = Message::BlockchainRequest(request).serialize();
            self.io_interface
                .send_message(peer_index, buffer)
                .await
                .unwrap();
        }
    }
    pub async fn process_incoming_block_hash(
        &mut self,
        block_hash: SaitoHash,
        peer_index: PeerIndex,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        mempool_lock: Arc<RwLock<Mempool>>,
    ) -> Option<()> {
        trace!(
            "processing incoming block hash : {:?} from peer : {:?}",
            block_hash.to_hex(),
            peer_index
        );
        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let block_exists;
        let my_public_key;
        {
            let (blockchain, _blockchain_) = lock_for_read!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            let (mempool, _mempool_) = lock_for_read!(mempool_lock, LOCK_ORDER_MEMPOOL);
            let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);

            block_exists = blockchain.is_block_indexed(block_hash)
                || mempool.blocks_queue.iter().any(|b| b.hash == block_hash);
            my_public_key = wallet.public_key;
        }
        if block_exists {
            debug!(
                "block : {:?} already exists in chain. not fetching",
                block_hash.to_hex()
            );
            return None;
        }
        let url;
        {
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

            if let Some(peer) = peers.index_to_peers.get(&peer_index) {
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
                warn!("peer : {:?} is not in peer list", peer_index);
                return None;
            }
        }

        debug!(
            "fetching block for incoming hash : {:?}",
            block_hash.to_hex()
        );
        let (mut blockchain, _blockchain_) =
            lock_for_write!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        blockchain.mark_as_fetching(block_hash);
        if self
            .io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
            .is_err()
        {
            // failed fetching block from peer
            warn!(
                "failed fetching block : {:?} for block hash. so unmarking block as fetching",
                block_hash.to_hex()
            );
            blockchain.unmark_as_fetching(&block_hash);
        }
        Some(())
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let (configs, _configs_) = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);

        configs
            .get_peer_configs()
            .clone()
            .drain(..)
            .for_each(|config| {
                self.static_peer_configs.push((config, 0));
            });
        if !self.static_peer_configs.is_empty() {
            self.static_peer_configs.get_mut(0).unwrap().0.is_main = true;
        }
        trace!("static peers : {:?}", self.static_peer_configs);
    }

    pub async fn connect_to_static_peers(&mut self) {
        let current_time = self.time_keeper.get_timestamp_in_ms();
        for (peer, reconnect_after) in &mut self.static_peer_configs {
            trace!("connecting to peer : {:?}", peer);
            if *reconnect_after > current_time {
                continue;
            }
            *reconnect_after = current_time + PEER_RECONNECT_WAIT_PERIOD;
            self.io_interface
                .connect_to_peer(peer.clone())
                .await
                .unwrap();
        }
    }

    pub async fn send_pings(&mut self) {
        let current_time = self.time_keeper.get_timestamp_in_ms();
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);
        for (_, peer) in peers.index_to_peers.iter_mut() {
            peer.send_ping(current_time, &self.io_interface).await;
        }
    }

    pub async fn update_peer_timer(&mut self, peer_index: PeerIndex) {
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            return;
        }
        let peer = peer.unwrap();
        peer.last_msg_at = self.time_keeper.get_timestamp_in_ms();
    }
}

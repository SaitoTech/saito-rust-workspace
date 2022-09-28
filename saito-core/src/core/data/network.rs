use std::fmt::Debug;
use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, trace};

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::{Configuration, PeerConfig};
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::msg::message::Message;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

#[derive(Debug)]
pub struct Network {
    // TODO : manage peers from network
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    static_peer_configs: Vec<PeerConfig>,
    pub wallet: Arc<RwLock<Wallet>>,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        wallet: Arc<RwLock<Wallet>>,
    ) -> Network {
        Network {
            peers,
            io_interface: io_handler,
            static_peer_configs: Default::default(),
            wallet,
        }
    }
    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", hex::encode(&block.hash));

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node

        {
            log_read_lock_request!("peers");
            let peers = self.peers.read().await;
            log_read_lock_receive!("peers");
            for (index, peer) in peers.index_to_peers.iter() {
                if !peer.handshake_done {
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

        debug!("sending block : {:?} to peers", hex::encode(&block.hash));
        let message = Message::BlockHeaderHash(block.hash);
        self.io_interface
            .send_message_to_all(message.serialize(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, transaction: &Transaction) {
        trace!(
            "propagating transaction : {:?}",
            hex::encode(transaction.signature)
        );

        // TODO : return if tx is not valid

        log_read_lock_request!("peers");
        let peers = self.peers.read().await;
        log_read_lock_receive!("peers");
        log_read_lock_request!("wallet");
        let wallet = self.wallet.read().await;
        log_read_lock_receive!("wallet");
        for (index, peer) in peers.index_to_peers.iter() {
            if transaction.is_in_path(&peer.public_key) {
                continue;
            }
            let mut transaction = transaction.clone();
            transaction.add_hop(&wallet, peer.public_key);
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
            hex::encode(block_hash),
            hex::encode(public_key)
        );
        let peer_index;
        let url;
        {
            log_read_lock_request!("peers");
            let peers = self.peers.read().await;
            log_read_lock_receive!("peers");
            let peer = peers.find_peer_by_address(public_key);
            if peer.is_none() {
                debug!("a = {:?}", peers.address_to_peers.len());
                todo!()
            }
            let peer = peer.unwrap();
            url = peer.get_block_fetch_url(block_hash);
            peer_index = peer.index;
        }

        self.io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
    }
    pub async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        log_read_lock_request!("peers");
        let peers = self.peers.read().await;
        log_read_lock_receive!("peers");
        let result = peers.find_peer_by_index(peer_index);

        if result.is_some() {
            let peer = result.unwrap();

            if peer.static_peer_config.is_some() {
                // This means the connection has been initiated from this side, therefore we must
                // try to re-establish the connection again
                // TODO : Add a delay so that there won't be a runaway issue with connects and
                // disconnects, check the best place to add (here or network_controller)
                info!(
                    "Static peer disconnected, reconnecting .., Peer ID = {}, Public Key = {:?}",
                    peer.index,
                    hex::encode(peer.public_key)
                );

                self.io_interface
                    .connect_to_peer(peer.static_peer_config.as_ref().unwrap().clone())
                    .await
                    .unwrap();

                self.static_peer_configs
                    .push(peer.static_peer_config.as_ref().unwrap().clone());
            } else {
                info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                    peer.index, hex::encode(peer.public_key));
            }
        } else {
            todo!("Handle the unknown peer disconnect");
        }
    }
    pub async fn handle_new_peer(
        &mut self,
        peer_data: Option<data::configuration::PeerConfig>,
        peer_index: u64,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        log_write_lock_request!("peers");
        let mut peers = self.peers.write().await;
        log_write_lock_receive!("peers");
        let mut peer = Peer::new(peer_index);
        peer.static_peer_config = peer_data;

        if peer.static_peer_config.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(&self.io_interface, wallet.clone(), configs.clone())
                .await
                .unwrap();
        } else {
            self.static_peer_configs
                .retain(|config| config != peer.static_peer_config.as_ref().unwrap());
        }

        peers.index_to_peers.insert(peer_index, peer);
        info!("new peer added : {:?}", peer_index);
    }
    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        log_write_lock_request!("peers");
        let mut peers = self.peers.write().await;
        log_write_lock_receive!("peers");
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        peer.handle_handshake_challenge(challenge, &self.io_interface, wallet.clone(), configs)
            .await
            .unwrap();
    }
    pub async fn handle_handshake_response(
        &self,
        peer_index: u64,
        response: HandshakeResponse,
        wallet: Arc<RwLock<Wallet>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        debug!("received handshake response");
        log_write_lock_request!("peers");
        let mut peers = self.peers.write().await;
        log_write_lock_receive!("peers");
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        peer.handle_handshake_response(response, &self.io_interface, wallet.clone())
            .await
            .unwrap();
        if peer.handshake_done {
            debug!(
                "peer : {:?} handshake successful for peer : {:?}",
                peer.index,
                hex::encode(peer.public_key)
            );
            let public_key = peer.public_key;
            peers.address_to_peers.insert(public_key, peer_index);
            // start block syncing here
            self.request_blockchain_from_peer(peer_index, blockchain.clone())
                .await;
        }
    }
    pub async fn handle_handshake_completion(
        &self,
        peer_index: u64,
        response: HandshakeCompletion,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        debug!("received handshake completion");
        let public_key;
        {
            log_write_lock_request!("peers");
            let mut peers = self.peers.write().await;
            log_write_lock_receive!("peers");
            let peer = peers.index_to_peers.get_mut(&peer_index);
            if peer.is_none() {
                todo!()
            }
            let peer = peer.unwrap();
            let _result = peer
                .handle_handshake_completion(response, &self.io_interface)
                .await;
            if !peer.handshake_done {
                return;
            }
            public_key = peer.public_key;
            peers.address_to_peers.insert(public_key, peer_index);
        }

        debug!(
            "peer : {:?} handshake successful for peer : {:?}",
            peer_index,
            hex::encode(public_key)
        );
        // start block syncing here
        self.request_blockchain_from_peer(peer_index, blockchain.clone())
            .await;
    }
    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        debug!("requesting blockchain from peer : {:?}", peer_index);

        // TODO : should this be moved inside peer ?
        let request;
        {
            log_read_lock_request!("blockchain");
            let blockchain = blockchain.read().await;
            log_read_lock_receive!("blockchain");
            request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id: blockchain.get_fork_id().clone(),
            };
        }

        let buffer = Message::BlockchainRequest(request).serialize();
        self.io_interface
            .send_message(peer_index, buffer)
            .await
            .unwrap();
    }
    pub async fn process_incoming_block_hash(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        blockchain: Arc<RwLock<Blockchain>>,
    ) {
        let block_exists;
        {
            log_read_lock_request!("blockchain");
            let blockchain = blockchain.read().await;
            log_read_lock_receive!("blockchain");
            block_exists = blockchain.is_block_indexed(block_hash);
        }
        let url;
        {
            log_read_lock_request!("peers");
            let peers = self.peers.read().await;
            log_read_lock_receive!("peers");
            let peer = peers
                .index_to_peers
                .get(&peer_index)
                .expect("peer not found");
            url = peer.get_block_fetch_url(block_hash);
        }
        if !block_exists {
            self.io_interface
                .fetch_block_from_peer(block_hash, peer_index, url)
                .await
                .unwrap();
        }
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        self.static_peer_configs = configs.read().await.get_peer_configs().clone();
    }

    pub async fn connect_to_static_peers(&mut self) {
        trace!("connect to static peers",);

        for peer in &self.static_peer_configs {
            self.io_interface
                .connect_to_peer(peer.clone())
                .await
                .unwrap();
        }
        trace!("connected to peers");
    }
}

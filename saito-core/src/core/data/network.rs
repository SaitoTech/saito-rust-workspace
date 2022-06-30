use std::io::Error;
use std::sync::Arc;

use log::{debug, info, trace};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::msg::message::Message;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

pub struct Network {
    // TODO : manage peers from network
    peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
    ) -> Network {
        Network {
            peers,
            io_interface: io_handler,
        }
    }
    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", hex::encode(&block.hash));

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node
        if block.source_connection_id.is_some() {
            trace!("waiting for the peers read lock");
            let peers = self.peers.read().await;
            trace!("acquired the peers read lock");
            let peer = peers
                .address_to_peers
                .get(&block.source_connection_id.unwrap());
            if peer.is_some() {
                excluded_peers.push(*peer.unwrap());
            }
        }
        debug!("sending block : {:?} to peers", hex::encode(&block.hash));
        let message = Message::BlockHeaderHash(block.hash);
        self.io_interface
            .send_message_to_all(message.serialize(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, _transaction: &Transaction) {
        debug!("propagating transaction");
        todo!()
    }

    pub async fn fetch_missing_block(
        &self,
        block_hash: SaitoHash,
        public_key: &SaitoPublicKey,
    ) -> Result<(), Error> {
        debug!(
            "fetch missing block : block : {:?} from : {:?}",
            block_hash, public_key
        );
        let peer_index;
        let url;
        {
            let peers = self.peers.read().await;
            let peer = peers.find_peer_by_address(public_key);
            let peer = peer.unwrap();
            url = peer.get_block_fetch_url(block_hash);
            peer_index = peer.peer_index;
        }

        self.io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
    }
    pub async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);
        let peers = self.peers.read().await;
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
                    peer.peer_index,
                    hex::encode(peer.peer_public_key)
                );

                self.io_interface
                    .connect_to_peer(peer.static_peer_config.as_ref().unwrap().clone())
                    .await
                    .unwrap();
            } else {
                info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                    peer.peer_index, hex::encode(peer.peer_public_key));
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
        configs: Arc<RwLock<Configuration>>,
    ) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        trace!("waiting for the peers write lock");
        let mut peers = self.peers.write().await;
        trace!("acquired the peers write lock");
        let mut peer = Peer::new(peer_index);
        peer.static_peer_config = peer_data;

        if peer.static_peer_config.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(&self.io_interface, wallet.clone(), configs.clone())
                .await
                .unwrap();
        }

        peers.index_to_peers.insert(peer_index, peer);
        info!("new peer added : {:?}", peer_index);
    }
    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Configuration>>,
    ) {
        let mut peers = self.peers.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        peer.handle_handshake_challenge(
            challenge,
            &self.io_interface,
            wallet.clone(),
            configs.clone(),
        )
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
        let mut peers = self.peers.write().await;
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
                peer.peer_index,
                hex::encode(peer.peer_public_key)
            );
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
        let mut peers = self.peers.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        let _result = peer
            .handle_handshake_completion(response, &self.io_interface)
            .await;
        if peer.handshake_done {
            debug!(
                "peer : {:?} handshake successful for peer : {:?}",
                peer.peer_index,
                hex::encode(peer.peer_public_key)
            );
            // start block syncing here
            self.request_blockchain_from_peer(peer_index, blockchain.clone())
                .await;
        }
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
            let blockchain = blockchain.read().await;
            request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id: blockchain.get_fork_id(),
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
            let blockchain = blockchain.read().await;
            block_exists = blockchain.is_block_indexed(block_hash);
        }
        let url;
        {
            let peers = self.peers.read().await;
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
    pub async fn connect_to_static_peers(&mut self, configs: Arc<RwLock<Configuration>>) {
        debug!("connect to peers from config",);
        trace!("waiting for the configs read lock");
        let configs = configs.read().await;
        trace!("acquired the configs read lock");

        for peer in &configs.peers {
            self.io_interface
                .connect_to_peer(peer.clone())
                .await
                .unwrap();
        }
        debug!("connected to peers");
    }
}

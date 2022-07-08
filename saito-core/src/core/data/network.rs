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
use crate::core::data::peer::{Peer, PeerConnection};
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

pub struct Network {
    // TODO : manage peers from network
    peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Wallet>>,
    configs: Arc<RwLock<Configuration>>,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Configuration>>,
    ) -> Network {
        Network {
            peers,
            io_interface: io_handler,
            blockchain,
            wallet,
            configs,
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

    pub async fn handle_new_peer(
        &mut self,
        peer_data: Option<data::configuration::PeerConfig>,
        peer_index: u64,
        connection: &mut impl PeerConnection,
    ) {
        // TODO : if an incoming peer is same as static peer, handle the scenario
        debug!("handing new peer : {:?}", peer_index);
        trace!("waiting for the peers write lock");

        let block_fetch_url;
        {
            block_fetch_url = self.configs.read().await.get_block_fetch_url();
        }

        let mut peer = Peer::new(
            self.blockchain.clone(),
            self.wallet.clone(),
            block_fetch_url,
            peer_index,
        );

        peer.static_peer_config = peer_data;

        if peer.static_peer_config.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(connection).await.unwrap();
        }

        {
            let mut peers = self.peers.write().await;
            trace!("acquired the peers write lock");
            peers.index_to_peers.insert(peer_index, peer);
            info!("new peer added : {:?}", peer_index);
        }
    }

    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        connection: &mut impl PeerConnection,
    ) {
        let mut peers = self.peers.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        peer.handle_handshake_challenge(challenge, connection)
            .await
            .unwrap();
    }

    pub async fn handle_handshake_response(
        &self,
        peer_index: u64,
        response: HandshakeResponse,
        connection: &mut impl PeerConnection,
    ) {
        debug!("received handshake response");
        let mut peers = self.peers.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }

        let peer = peer.unwrap();
        peer.handle_handshake_response(response, connection)
            .await
            .unwrap();

        if peer.handshake_done {
            debug!(
                "peer : {:?} handshake successful for peer : {:?}",
                peer.peer_index,
                hex::encode(peer.peer_public_key)
            );
            // start block syncing here
            self.request_blockchain_from_peer(peer_index, connection)
                .await;
        }
    }

    pub async fn handle_handshake_completion(
        &self,
        peer_index: u64,
        response: HandshakeCompletion,
        connection: &mut impl PeerConnection,
    ) {
        debug!("received handshake completion");
        let mut peers = self.peers.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            todo!()
        }
        let peer = peer.unwrap();
        let result = peer.handle_handshake_completion(response).await;
        if peer.handshake_done {
            debug!(
                "peer : {:?} handshake successful for peer : {:?}",
                peer.peer_index,
                hex::encode(peer.peer_public_key)
            );
            // start block syncing here
            peer.request_blockchain(connection).await;
        }
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

    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        connection: &mut impl PeerConnection,
    ) {
        debug!("requesting blockchain from peer : {:?}", peer_index);

        // TODO : should this be moved inside peer ?
        let request;
        {
            let blockchain = self.blockchain.read().await;
            request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id: blockchain.get_fork_id(),
            };
        }

        let buffer = Message::BlockchainRequest(request).serialize();
        connection.send_message(buffer).await.unwrap();
    }

    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        peer_index: u64,
        connection: &mut impl PeerConnection,
    ) {
        debug!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            hex::encode(request.latest_block_hash),
            hex::encode(request.fork_id),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        let blockchain = self.blockchain.read().await;

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

            connection.send_message(buffer).await.unwrap();
        }
    }

    pub async fn process_incoming_block_hash(&self, block_hash: SaitoHash, peer_index: u64) {
        let block_exists;
        {
            let blockchain = self.blockchain.read().await;
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

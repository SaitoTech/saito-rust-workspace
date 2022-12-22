use std::fmt::Debug;
use std::io::Error;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

use crate::common::defs::{
    push_lock, PeerIndex, SaitoHash, SaitoPublicKey, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_PEERS, LOCK_ORDER_WALLET,
};
use crate::common::interface_io::InterfaceIO;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::{Configuration, PeerConfig};
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::data::msg::message::Message;
use crate::core::data::peer::Peer;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::{lock_for_read, lock_for_write};

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

        debug!("sending block : {:?} to peers", hex::encode(&block.hash));
        let message = Message::BlockHeaderHash(block.hash, block.id);
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

        let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

        let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
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
            hex::encode(block_hash),
            hex::encode(public_key)
        );
        let peer_index;
        let url;
        {
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

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
        let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

        let result = peers.find_peer_by_index(peer_index);

        if result.is_some() {
            let peer = result.unwrap();

            if peer.static_peer_config.is_some() {
                // This means the connection has been initiated from this side, therefore we must
                // try to re-establish the connection again
                // TODO : Add a delay so that there won't be a runaway issue with connects and
                // disconnects, check the best place to add (here or network_controller)
                info!(
                    "Static peer disconnected, reconnecting .., Peer ID = {}",
                    peer.index
                );

                self.io_interface
                    .connect_to_peer(peer.static_peer_config.as_ref().unwrap().clone())
                    .await
                    .unwrap();

                self.static_peer_configs
                    .push(peer.static_peer_config.as_ref().unwrap().clone());
            } else {
                info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                    peer.index, hex::encode(peer.public_key.as_ref().unwrap()));
            }
        } else {
            todo!("Handle the unknown peer disconnect");
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
            info!(
                "removing static peer config : {:?}",
                peer.static_peer_config.as_ref().unwrap()
            );
            self.static_peer_configs
                .retain(|config| config != peer.static_peer_config.as_ref().unwrap());
        }

        info!("new peer added : {:?}", peer_index);
        peers.index_to_peers.insert(peer_index, peer);
    }
    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

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
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        debug!("received handshake response");
        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            warn!("peer not found : {:?}", peer_index);
            todo!()
        }
        let peer = peer.unwrap();
        peer.handle_handshake_response(
            response,
            &self.io_interface,
            wallet.clone(),
            configs.clone(),
        )
        .await
        .unwrap();
        if peer.public_key.is_some() {
            debug!(
                "peer : {:?} handshake successful for peer : {:?}",
                peer.index,
                hex::encode(peer.public_key.as_ref().unwrap())
            );
            let public_key = peer.public_key.clone().unwrap();
            peers.address_to_peers.insert(public_key, peer_index);
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
        info!("requesting blockchain from peer : {:?}", peer_index);

        // TODO : should this be moved inside peer ?
        let request;
        {
            let (blockchain, _blockchain_) = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);

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
        peer_index: PeerIndex,
        blockchain: Arc<RwLock<Blockchain>>,
    ) -> Option<()> {
        let block_exists;
        {
            let (blockchain, _blockchain_) = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);

            block_exists = blockchain.is_block_indexed(block_hash);
        }
        if block_exists {
            return None;
        }
        let url;
        {
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

            let peer = peers
                .index_to_peers
                .get(&peer_index)
                .expect("peer not found");
            url = peer.get_block_fetch_url(block_hash);
        }
        self.io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
            .unwrap();
        Some(())
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    ) {
        let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);
        self.static_peer_configs = configs.get_peer_configs().clone();
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

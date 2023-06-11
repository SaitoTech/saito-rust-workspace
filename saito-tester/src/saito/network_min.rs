//minimal network interface

use std::fmt::Debug;
use std::io::Error;
use std::sync::Arc;

use log::{debug, info, trace, warn};
use tokio::sync::RwLock;

use saito_core::common::defs::{
    push_lock, PeerIndex, SaitoHash, SaitoPublicKey, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_PEERS, LOCK_ORDER_WALLET,
};
use saito_core::common::interface_io::{InterfaceEvent, InterfaceIO};
use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::msg::block_request::BlockchainRequest;
use saito_core::core::data::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::peer_service::PeerService;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};

#[derive(Debug)]
pub struct NetworkMin {
    // TODO : manage peers from network
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    static_peer_configs: Vec<PeerConfig>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl NetworkMin {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) -> NetworkMin {
        NetworkMin {
            peers,
            io_interface: io_handler,
            static_peer_configs: Default::default(),
            wallet,
            configs,
        }
    }
    
    pub async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        trace!("handling peer disconnect, peer_index = {}", peer_index);

        // calling here before removing the peer from collections
        self.io_interface
            .send_interface_event(InterfaceEvent::PeerConnectionDropped(peer_index));

        let (mut peers, _peers_) = lock_for_write!(self.peers, LOCK_ORDER_PEERS);

        let result = peers.find_peer_by_index(peer_index);

        if result.is_some() {
            let peer = result.unwrap();
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

                self.io_interface
                    .connect_to_peer(peer.static_peer_config.as_ref().unwrap().clone())
                    .await
                    .unwrap();

                self.static_peer_configs
                    .push(peer.static_peer_config.as_ref().unwrap().clone());
            } else {
                if peer.public_key.is_some() {
                    info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                    peer.index, hex::encode(peer.public_key.as_ref().unwrap()));
                }
            }

            if public_key.is_some() {
                peers.address_to_peers.remove(&public_key.unwrap());
            }
            peers.index_to_peers.remove(&peer_index);
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
            let data = peer.static_peer_config.as_ref().unwrap();

            self.static_peer_configs
                .retain(|config| config.host != data.host || config.port != data.port);
        }

        info!("new peer added : {:?}", peer_index);
        peers.index_to_peers.insert(peer_index, peer);
        info!("current peer count = {:?}", peers.index_to_peers.len());
        self.io_interface
            .send_interface_event(InterfaceEvent::PeerConnected(peer_index));
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
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
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
            //!
            
        }
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);
        self.static_peer_configs = configs.get_peer_configs().clone();
        if !self.static_peer_configs.is_empty() {
            self.static_peer_configs.get_mut(0).unwrap().is_main = true;
        }
        trace!("static peers : {:?}", self.static_peer_configs);
    }

    pub async fn connect_to_static_peers(&mut self) {
        // trace!(
        //     "connect to static peers : count = {:?}",
        //     self.static_peer_configs.len()
        // );

        for peer in &self.static_peer_configs {
            trace!("connecting to peer : {:?}", peer);
            self.io_interface
                .connect_to_peer(peer.clone())
                .await
                .unwrap();
        }
        // trace!("connected to peers");
    }
    }

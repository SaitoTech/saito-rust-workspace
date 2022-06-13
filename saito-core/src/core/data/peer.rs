use std::io::Error;
use std::sync::Arc;

use log::{debug, warn};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::configuration::Configuration;
use crate::core::data::crypto::{generate_random_bytes, sign, verify};
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::msg::message::Message;
use crate::core::data::wallet::Wallet;
use async_trait::async_trait;

#[async_trait]
pub trait PeerConnection {
    async fn send_message(&self, buffer: Vec<u8>);
}

#[derive(Debug, Clone)]
pub struct Peer {
    wallet: Arc<RwLock<Wallet>>,
    configs: Arc<RwLock<Configuration>>,
    pub peer_index: u64,
    pub peer_public_key: SaitoPublicKey,
    pub peer_block_fetch_url: String,
    // if this is None(), it means an incoming connection. else a connection which we started from the data from config file
    pub static_peer_config: Option<data::configuration::PeerConfig>,
    pub challenge_for_peer: Option<SaitoHash>,
    pub handshake_done: bool,
}

impl Peer {
    pub fn new(
        wallet: Arc<RwLock<Wallet>>,
        configs: Arc<RwLock<Configuration>>,
        peer_index: u64,
    ) -> Peer {
        Peer {
            wallet,
            configs,
            peer_index,
            peer_public_key: [0; 33],
            peer_block_fetch_url: "".to_string(),
            static_peer_config: None,
            challenge_for_peer: None,
            handshake_done: false,
        }
    }
    pub async fn initiate_handshake(
        &mut self,
        connection: &Box<dyn PeerConnection + Send + Sync>,
    ) -> Result<(), Error> {
        debug!("initiating handshake : {:?}", self.peer_index);
        let wallet = self.wallet.read().await;
        let block_fetch_url;
        {
            let configs = self.configs.read().await;
            block_fetch_url = configs.get_block_fetch_url();
        }
        let challenge = HandshakeChallenge {
            public_key: self.wallet.public_key,
            challenge: generate_random_bytes(32).try_into().unwrap(),
            block_fetch_url,
        };
        self.challenge_for_peer = Some(challenge.challenge);
        let message = Message::HandshakeChallenge(challenge);
        connection.send_message(message.serialize()).await;
        debug!("handshake challenge sent for peer: {:?}", self.peer_index);

        Ok(())
    }
    pub async fn handle_handshake_challenge(
        &mut self,
        challenge: HandshakeChallenge,
        connection: &Box<dyn PeerConnection + Send + Sync>,
    ) -> Result<(), Error> {
        debug!(
            "handling handshake challenge : {:?} with address : {:?}",
            self.peer_index,
            hex::encode(challenge.public_key)
        );
        let block_fetch_url;
        {
            let configs = self.configs.read().await;
            block_fetch_url = configs.get_block_fetch_url();
        }

        self.peer_public_key = challenge.public_key;
        self.peer_block_fetch_url = challenge.block_fetch_url;
        let wallet = self.wallet.read().await;
        let response = HandshakeResponse {
            public_key: wallet.public_key,
            signature: sign(&challenge.challenge.to_vec(), wallet.private_key),
            challenge: generate_random_bytes(32).try_into().unwrap(),
            block_fetch_url,
        };

        self.challenge_for_peer = Some(response.challenge);
        connection
            .send_message(Message::HandshakeResponse(response).serialize())
            .await;
        debug!("handshake response sent for peer: {:?}", self.peer_index);

        Ok(())
    }
    pub async fn handle_handshake_response(
        &mut self,
        response: HandshakeResponse,
        connection: &Box<dyn PeerConnection + Send + Sync>,
    ) -> Result<(), Error> {
        debug!(
            "handling handshake response :{:?} with address : {:?}",
            self.peer_index,
            hex::encode(response.public_key)
        );
        if self.challenge_for_peer.is_none() {
            warn!(
                "we don't have a challenge to verify for peer : {:?}",
                self.peer_index
            );
            // TODO : handle the scenario.
            todo!()
        }
        let sent_challenge = self.challenge_for_peer.unwrap();
        let result = verify(&sent_challenge, response.signature, response.public_key);
        if !result {
            warn!("handshake failed. signature is not valid");
            todo!()
        }
        self.challenge_for_peer = None;
        self.peer_public_key = response.public_key;
        self.peer_block_fetch_url = response.block_fetch_url;
        self.handshake_done = true;
        let wallet = wallet.read().await;
        let response = HandshakeCompletion {
            signature: sign(&response.challenge, wallet.private_key),
        };
        connection
            .send_message(Message::HandshakeCompletion(response).serialize())
            .await;

        debug!("handshake completion sent for peer: {:?}", self.peer_index);
        Ok(())
    }
    pub async fn handle_handshake_completion(
        &mut self,
        response: HandshakeCompletion,
    ) -> Result<(), Error> {
        debug!("handling handshake completion : {:?}", self.peer_index);
        if self.challenge_for_peer.is_none() {
            warn!(
                "we don't have a challenge to verify for peer : {:?}",
                self.peer_index
            );
            // TODO : handle the scenario.
            todo!()
        }
        let sent_challenge = self.challenge_for_peer.unwrap();
        let result = verify(&sent_challenge, response.signature, self.peer_public_key);
        if !result {
            warn!("handshake failed. signature is not valid");
            todo!()
        }
        self.challenge_for_peer = None;
        self.handshake_done = true;
        Ok(())
    }
    /// Since each peer have a different url for a block to be fetched, this function will generate the correct url from a given block hash
    ///
    /// # Arguments
    ///
    /// * `block_hash`: hash of the block to be fetched
    ///
    /// returns: String
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn get_block_fetch_url(&self, block_hash: SaitoHash) -> String {
        // TODO : generate the url with proper / escapes,etc...
        self.peer_block_fetch_url.to_string() + hex::encode(block_hash).as_str()
    }
}

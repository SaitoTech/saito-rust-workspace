use std::io::Error;
use std::sync::Arc;

use log::{debug, warn};
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::crypto::{generate_random_bytes, sign, verify};
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::msg::message::Message;
use crate::core::data::wallet::Wallet;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

#[async_trait]
pub trait PeerConnection {
    async fn send_message(&mut self, buffer: Vec<u8>) -> Result<(), Error>;
    fn get_peer_index(&self) -> u64;
}

#[derive(Debug, Clone)]
pub struct Peer {
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Wallet>>,
    local_block_fetch_url: String,
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
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        local_block_fetch_url: String,
        peer_index: u64,
    ) -> Peer {
        Peer {
            blockchain,
            wallet,
            local_block_fetch_url,
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
        connection: &mut impl PeerConnection,
    ) -> Result<(), Error> {
        debug!("initiating handshake : {:?}", self.peer_index);
        let wallet = self.wallet.read().await;
        let block_fetch_url = self.local_block_fetch_url.clone();

        let challenge = HandshakeChallenge {
            public_key: wallet.public_key,
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
        connection: &mut impl PeerConnection,
    ) -> Result<(), Error> {
        debug!(
            "handling handshake challenge : {:?} with address : {:?}",
            self.peer_index,
            hex::encode(challenge.public_key)
        );

        let block_fetch_url = self.local_block_fetch_url.clone();
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
        connection: &mut impl PeerConnection,
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
        let wallet = self.wallet.read().await;
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

    pub async fn request_blockchain(&self, connection: &mut impl PeerConnection) {
        debug!("requesting blockchain from peer : {:?}", self.peer_index);

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
        connection.send_message(buffer).await;
    }

    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        connection: &mut impl PeerConnection,
    ) {
        debug!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            hex::encode(request.latest_block_hash),
            hex::encode(request.fork_id),
            self.peer_index
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

            connection.send_message(buffer).await;
        }
    }

    pub async fn process_incoming_block_hash(
        &self,
        block_hash: SaitoHash,
        connection: &mut impl PeerConnection,
    ) {
        let block_exists;
        {
            let blockchain = self.blockchain.read().await;
            block_exists = blockchain.is_block_indexed(block_hash);
        }
        let url = self.get_block_fetch_url(block_hash);

        if !block_exists {
            self.fetch_block(block_hash).await;
        }
    }

    pub async fn fetch_block(&self, block_hash: SaitoHash) {
        let url = self.get_block_fetch_url(block_hash);
        let peer_index = self.peer_index;

        debug!("fetching block : {:?}, peer index {:?}", url, peer_index);
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

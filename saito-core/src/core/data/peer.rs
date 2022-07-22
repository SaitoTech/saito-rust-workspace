use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::{debug, warn};
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::{Configuration, PeerConfig};
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
    pub static_peer_config: Option<data::configuration::PeerConfig>,
    pub challenge_for_peer: Option<SaitoHash>,
    pub handshake_done: bool,
    event_sender: Sender<NetworkEvent>,
}

impl Peer {
    pub fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        local_block_fetch_url: String,
        peer_index: u64,
        config: Option<PeerConfig>,
        event_sender: Sender<NetworkEvent>,
    ) -> Peer {
        Peer {
            blockchain,
            wallet,
            local_block_fetch_url,
            peer_index,
            peer_public_key: [0; 33],
            peer_block_fetch_url: "".to_string(),
            static_peer_config: config,
            challenge_for_peer: None,
            handshake_done: false,
            event_sender,
        }
    }

    pub fn get_connection_url(&self) -> Option<String> {
        if self.static_peer_config.is_some() {
            let config = self.static_peer_config.as_ref().unwrap();
            let mut protocol: String = String::from("ws");
            if config.protocol == "https" {
                protocol = String::from("wss");
            }

            let url = protocol
                + "://"
                + config.host.as_str()
                + ":"
                + config.port.to_string().as_str()
                + "/wsopen";

            return Some(url);
        }

        return None;
    }

    pub fn get_block_fetch_url(&self, block_hash: SaitoHash) -> String {
        // TODO : generate the url with proper / escapes,etc...
        self.peer_block_fetch_url.to_string() + hex::encode(block_hash).as_str()
    }

    pub async fn process_incoming_message(&mut self, message: Message) -> Result<Vec<u8>, Error> {
        debug!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            self.peer_index
        );

        return match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                self.handle_handshake_challenge(challenge).await
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                self.handle_handshake_response(response).await
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                self.handle_handshake_completion(response).await
            }
            Message::BlockchainRequest(request) => {
                debug!("received blockchain request");
                self.process_incoming_blockchain_request(request).await
            }
            Message::BlockHeaderHash(hash) => {
                debug!("received block hash");
                self.process_incoming_block_hash(hash).await
            }
            Message::ApplicationMessage(_) => Err(Error::from(ErrorKind::InvalidData)),
            Message::Block(_) => Err(Error::from(ErrorKind::InvalidData)),
            Message::Transaction(_) => Err(Error::from(ErrorKind::InvalidData)),
        };
    }

    pub async fn initiate_handshake(&mut self) -> Result<Vec<u8>, Error> {
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
        debug!("handshake challenge sent for peer: {:?}", self.peer_index);
        Ok(message.serialize())
    }

    async fn handle_handshake_challenge(
        &mut self,
        challenge: HandshakeChallenge,
    ) -> Result<Vec<u8>, Error> {
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
        debug!("handshake response sent for peer: {:?}", self.peer_index);
        Ok(Message::HandshakeResponse(response).serialize())
    }

    async fn handle_handshake_response(
        &mut self,
        response: HandshakeResponse,
    ) -> Result<Vec<u8>, Error> {
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

        debug!("handshake completion sent for peer: {:?}", self.peer_index);
        Ok(Message::HandshakeCompletion(response).serialize())
    }

    async fn handle_handshake_completion(
        &mut self,
        response: HandshakeCompletion,
    ) -> Result<Vec<u8>, Error> {
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
        return self.request_blockchain().await;
    }

    async fn request_blockchain(&self) -> Result<Vec<u8>, Error> {
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

        Ok(Message::BlockchainRequest(request).serialize())
    }

    async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
    ) -> Result<Vec<u8>, Error> {
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
        let mut buffer: Vec<u8> = Default::default();

        for i in last_shared_ancestor..(blockchain.blockring.get_latest_block_id() + 1) {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i);
            if block_hash == [0; 32] {
                // TODO : can the block hash not be in the ring if we are going through the longest chain ?
                continue;
            }

            buffer.append(&mut Message::BlockHeaderHash(block_hash).serialize());
        }

        Ok(buffer)
    }

    async fn process_incoming_block_hash(&self, block_hash: SaitoHash) -> Result<Vec<u8>, Error> {
        let block_exists;
        {
            let blockchain = self.blockchain.read().await;
            block_exists = blockchain.is_block_indexed(block_hash);
        }

        if !block_exists {
            let url = self.get_block_fetch_url(block_hash);
            self.fetch_missing_block(url, block_hash, 0);
        }

        let buffer: Vec<u8> = Default::default();
        Ok(buffer)
    }

    pub async fn fetch_missing_block(&self, url: String, block_hash: SaitoHash, request_id: u64) {
        self.send_event(NetworkEvent::BlockFetchRequest {
            block_hash,
            peer_index: self.peer_index,
            url: url.clone(),
            request_id,
        });
    }

    pub async fn send_buffer(&self, buffer: Vec<u8>) {
        self.send_event(NetworkEvent::OutgoingNetworkMessage {
            buffer,
            peer_index: self.peer_index,
        })
        .await;
    }

    async fn send_event(&self, event: NetworkEvent) {
        self.event_sender.send(event).await;
    }
}

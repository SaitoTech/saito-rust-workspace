use std::io::Error;
use std::sync::Arc;

use log::{debug, warn};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoSignature};
use crate::common::handle_io::HandleIo;
use crate::core::data;
use crate::core::data::crypto::{generate_random_bytes, sign, verify};
use crate::core::data::handshake::{HandshakeChallenge, HandshakeCompletion, HandshakeResponse};
use crate::core::data::message::Message;
use crate::core::data::serialize::Serialize;
use crate::core::data::wallet::Wallet;

#[derive(Debug, Clone)]
pub struct Peer {
    pub peer_index: u64,
    pub peer_public_key: SaitoPublicKey,
    pub static_peer_config: Option<data::configuration::Peer>,
    pub challenge_for_peer: Option<SaitoHash>,
}

impl Peer {
    pub fn new(peer_index: u64) -> Peer {
        Peer {
            peer_index,
            peer_public_key: [0; 33],
            static_peer_config: None,
            challenge_for_peer: None,
        }
    }
    pub async fn initiate_handshake(
        &mut self,
        io_handler: &Box<dyn HandleIo + Send + Sync>,
        wallet: Arc<RwLock<Wallet>>,
    ) -> Result<(), Error> {
        debug!("initiating handshake : {:?}", self.peer_index);
        let wallet = wallet.read().await;

        let challenge = HandshakeChallenge {
            public_key: wallet.publickey,
            challenge: generate_random_bytes(32).try_into().unwrap(),
        };
        self.challenge_for_peer = Some(challenge.challenge);
        let message = Message::HandshakeChallenge(challenge);
        io_handler
            .send_message(self.peer_index, message.serialize())
            .await
            .unwrap();
        Ok(())
    }
    pub async fn handle_handshake_challenge(
        &mut self,
        challenge: HandshakeChallenge,
        io_handler: &Box<dyn HandleIo + Send + Sync>,
        wallet: Arc<RwLock<Wallet>>,
    ) -> Result<(), Error> {
        debug!(
            "handling handshake challenge : {:?} with address : {:?}",
            self.peer_index,
            hex::encode(challenge.public_key)
        );

        self.peer_public_key = challenge.public_key;
        let wallet = wallet.read().await;
        let response = HandshakeResponse {
            public_key: wallet.publickey,
            signature: sign(&challenge.challenge.to_vec(), wallet.privatekey),
            challenge: generate_random_bytes(32).try_into().unwrap(),
        };

        self.challenge_for_peer = Some(response.challenge);
        io_handler
            .send_message(
                self.peer_index,
                Message::HandshakeResponse(response).serialize(),
            )
            .await
            .unwrap();
        Ok(())
    }
    pub async fn handle_handshake_response(
        &mut self,
        response: HandshakeResponse,
        io_handler: &Box<dyn HandleIo + Send + Sync>,
        wallet: Arc<RwLock<Wallet>>,
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
        let wallet = wallet.read().await;
        let response = HandshakeCompletion {
            signature: sign(&response.challenge, wallet.privatekey),
        };
        io_handler
            .send_message(
                self.peer_index,
                Message::HandshakeCompletion(response).serialize(),
            )
            .await
            .unwrap();
        Ok(())
    }
    pub async fn handle_handshake_completion(
        &mut self,
        response: HandshakeCompletion,
        io_handler: &Box<dyn HandleIo + Send + Sync>,
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
        Ok(())
    }
    pub fn get_block_fetch_url(&self, block_hash: SaitoHash) -> String {
        let config = self.static_peer_config.as_ref().unwrap();
        format!(
            "{:?}://{:?}:{:?}/block/{:?}",
            config.protocol,
            config.host,
            config.port,
            hex::encode(block_hash)
        )
    }
}

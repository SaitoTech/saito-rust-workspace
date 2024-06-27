use std::cmp::Ordering;
use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::{debug, info, warn};
use tokio::sync::RwLock;

use crate::core::consensus::peer_service::PeerService;
use crate::core::consensus::wallet::Wallet;
use crate::core::defs::{PrintForLog, SaitoHash, SaitoPublicKey, Timestamp, WS_KEEP_ALIVE_PERIOD};
use crate::core::io::interface_io::{InterfaceEvent, InterfaceIO};
use crate::core::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::msg::message::Message;
use crate::core::process::version::Version;
use crate::core::util;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::{generate_random_bytes, sign, verify};
use crate::core::util::rate_limiter::RateLimiter;

#[derive(Debug, Clone)]
pub struct Peer {
    pub index: u64,
    pub public_key: Option<SaitoPublicKey>,
    pub block_fetch_url: String,
    // if this is None(), it means an incoming connection. else a connection which we started from the data from config file
    pub static_peer_config: Option<util::configuration::PeerConfig>,
    pub challenge_for_peer: Option<SaitoHash>,
    pub key_list: Vec<SaitoPublicKey>,
    pub services: Vec<PeerService>,
    pub last_msg_at: Timestamp,
    pub wallet_version: Version,
    pub core_version: Version,
    pub rate_limiter: RateLimiter
}

impl Peer {
    pub fn new(peer_index: u64) -> Peer {
        
        let mut rate_limiter = RateLimiter::new();
        rate_limiter.set_limit("key_list", 10, 60_000);  
        rate_limiter.set_limit("handshake_challenge", 5, 60_000); 

        Peer {
            index: peer_index,
            public_key: None,
            block_fetch_url: "".to_string(),
            static_peer_config: None,
            challenge_for_peer: None,
            key_list: vec![],
            services: vec![],
            last_msg_at: 0,
            wallet_version: Default::default(),
            core_version: Default::default(),
            rate_limiter: rate_limiter
        }
        
    }
    pub async fn initiate_handshake(
        &mut self,
        io_handler: &(dyn InterfaceIO + Send + Sync),
    ) -> Result<(), Error> {
        debug!("initiating handshake : {:?}", self.index);

        let challenge = HandshakeChallenge {
            challenge: generate_random_bytes(32).try_into().unwrap(),
        };
        self.challenge_for_peer = Some(challenge.challenge);
        let message = Message::HandshakeChallenge(challenge);
        io_handler
            .send_message(self.index, message.serialize().as_slice())
            .await
            .unwrap();
        debug!("handshake challenge sent for peer: {:?}", self.index);

        Ok(())
    }

    pub fn can_make_request(&mut self, action: &str, current_time: u64) -> bool {
        self.rate_limiter.can_make_request(action, current_time)
    }
    pub async fn handle_handshake_challenge(
        &mut self,
        challenge: HandshakeChallenge,
        io_handler: &(dyn InterfaceIO + Send + Sync),
        wallet_lock: Arc<RwLock<Wallet>>,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) -> Result<(), Error> {
        debug!("handling handshake challenge : {:?}", self.index,);
        let block_fetch_url;
        let is_lite;
        {
            let configs = configs_lock.read().await;

            is_lite = configs.is_spv_mode();
            if is_lite {
                block_fetch_url = "".to_string();
            } else {
                block_fetch_url = configs.get_block_fetch_url();
            }
        }

        let wallet = wallet_lock.read().await;
        let response = HandshakeResponse {
            public_key: wallet.public_key,
            signature: sign(challenge.challenge.as_slice(), &wallet.private_key),
            challenge: generate_random_bytes(32).try_into().unwrap(),
            is_lite,
            block_fetch_url,
            services: io_handler.get_my_services(),
            wallet_version: wallet.wallet_version,
            core_version: wallet.core_version,
        };

        self.challenge_for_peer = Some(response.challenge);
        io_handler
            .send_message(
                self.index,
                Message::HandshakeResponse(response).serialize().as_slice(),
            )
            .await
            .unwrap();
        debug!("handshake response sent for peer: {:?}", self.index);

        Ok(())
    }
    pub async fn handle_handshake_response(
        &mut self,
        response: HandshakeResponse,
        io_handler: &(dyn InterfaceIO + Send + Sync),
        wallet_lock: Arc<RwLock<Wallet>>,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) -> Result<(), Error> {
        debug!(
            "handling handshake response :{:?} with address : {:?}",
            self.index,
            response.public_key.to_base58()
        );
        if !response.core_version.is_set() {
            debug!(
                "core version is not set in handshake response. expected : {:?}",
                wallet_lock.read().await.core_version
            );
            io_handler.disconnect_from_peer(self.index).await?;
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        if self.challenge_for_peer.is_none() {
            warn!(
                "we don't have a challenge to verify for peer : {:?}",
                self.index
            );
            io_handler.disconnect_from_peer(self.index).await?;
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        // TODO : validate block fetch URL
        let sent_challenge = self.challenge_for_peer.unwrap();
        let result = verify(&sent_challenge, &response.signature, &response.public_key);
        if !result {
            warn!(
                "handshake failed. signature is not valid. sig : {:?} challenge : {:?} key : {:?}",
                sent_challenge.to_hex(),
                response.signature.to_hex(),
                response.public_key.to_base58()
            );
            io_handler.disconnect_from_peer(self.index).await?;
            return Err(Error::from(ErrorKind::InvalidInput));
        }

        let block_fetch_url;
        let is_lite;
        {
            let configs = configs_lock.read().await;

            is_lite = configs.is_spv_mode();
            if is_lite {
                block_fetch_url = "".to_string();
            } else {
                block_fetch_url = configs.get_block_fetch_url();
            }
        }
        let wallet = wallet_lock.read().await;

        if !wallet
            .core_version
            .is_same_minor_version(&response.core_version)
        {
            warn!("peer : {:?} core version is not compatible. current core version : {:?} peer core version : {:?}",
                self.index, wallet.core_version, response.core_version);
            io_handler.send_interface_event(InterfaceEvent::NewVersionDetected(
                self.index,
                response.wallet_version,
            ));
            io_handler.disconnect_from_peer(self.index).await?;
            return Err(Error::from(ErrorKind::InvalidInput));
        }

        self.challenge_for_peer = None;
        self.public_key = Some(response.public_key);
        self.block_fetch_url = response.block_fetch_url;
        self.services = response.services;
        self.wallet_version = response.wallet_version;
        self.core_version = response.core_version;

        info!(
            "my version : {:?} peer version : {:?}",
            wallet.wallet_version, response.wallet_version
        );
        if wallet.wallet_version < response.wallet_version {
            io_handler.send_interface_event(InterfaceEvent::NewVersionDetected(
                self.index,
                response.wallet_version,
            ));
        }

        if self.static_peer_config.is_none() {
            // this is only called in initiator's side.
            // [1. A:challenge -> 2. B:response -> 3. A : response|B verified -> 4. B: A verified]
            // we only need to send a response for response is in above stage 3 (meaning the challenger).

            let response = HandshakeResponse {
                public_key: wallet.public_key,
                signature: sign(&response.challenge, &wallet.private_key),
                is_lite,
                block_fetch_url: block_fetch_url.to_string(),
                challenge: generate_random_bytes(32).try_into().unwrap(),
                services: io_handler.get_my_services(),
                wallet_version: wallet.wallet_version,
                core_version: wallet.core_version,
            };
            io_handler
                .send_message(
                    self.index,
                    Message::HandshakeResponse(response).serialize().as_slice(),
                )
                .await
                .unwrap();
            debug!("handshake response sent for peer: {:?}", self.index);
        } else {
            info!(
                "handshake completed for peer : {:?}",
                self.public_key.as_ref().unwrap().to_base58()
            );
        }
        io_handler.send_interface_event(InterfaceEvent::PeerHandshakeComplete(self.index));

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
    pub fn get_block_fetch_url(
        &self,
        block_hash: SaitoHash,
        lite: bool,
        my_public_key: SaitoPublicKey,
    ) -> String {
        // TODO : generate the url with proper / escapes,etc...
        if lite {
            self.block_fetch_url.to_string()
                + "/lite-block/"
                + block_hash.to_hex().as_str()
                + "/"
                + my_public_key.to_base58().as_str()
        } else {
            self.block_fetch_url.to_string() + "/block/" + block_hash.to_hex().as_str()
        }
    }
    pub async fn send_ping(
        &mut self,
        current_time: Timestamp,
        io_handler: &(dyn InterfaceIO + Send + Sync),
    ) {
        if self.last_msg_at + WS_KEEP_ALIVE_PERIOD < current_time {
            self.last_msg_at = current_time;
            // trace!("sending ping to peer : {:?}", self.index);
            io_handler
                .send_message(self.index, Message::Ping().serialize().as_slice())
                .await
                .unwrap();
        }
    }
    pub fn has_service(&self, service: String) -> bool {
        self.services.iter().any(|s| s.service == service)
    }
    pub fn is_main_peer(&self) -> bool {
        if self.static_peer_config.is_none() {
            return false;
        }
        self.static_peer_config.as_ref().unwrap().is_main
    }

    pub fn compare_version(&self, version: &Version) -> Option<Ordering> {
        // for peer versions, if the version is not set we still consider it as a valid peer
        // TODO : this could lead to an attack. need to provide different versions for different layer components
        if !version.is_set() || !self.wallet_version.is_set() {
            return Some(Ordering::Equal);
        }
        self.wallet_version.partial_cmp(version)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::consensus::peer::Peer;
    use crate::core::process::version::Version;
    use std::cmp::Ordering;

    #[test]
    fn peer_new_test() {
        let peer = Peer::new(1);

        assert_eq!(peer.index, 1);
        assert_eq!(peer.public_key, None);
        assert_eq!(peer.block_fetch_url, "".to_string());
        assert_eq!(peer.static_peer_config, None);
        assert_eq!(peer.challenge_for_peer, None);
    }

    #[test]
    fn peer_compare_test() {
        let peer_1 = Peer::new(1);
        let mut peer_2 = Peer::new(2);
        let mut peer_3 = Peer::new(3);
        let mut peer_4 = Peer::new(4);

        assert_eq!(peer_1.wallet_version, Version::new(0, 0, 0));

        peer_2.wallet_version = Version::new(0, 0, 1);

        peer_3.wallet_version = Version::new(0, 1, 0);

        peer_4.wallet_version = Version::new(1, 0, 0);

        assert_eq!(
            peer_1.compare_version(&peer_2.wallet_version),
            Some(Ordering::Equal)
        );
        assert_eq!(
            peer_2.compare_version(&peer_1.wallet_version),
            Some(Ordering::Equal)
        );

        assert_eq!(
            peer_3.compare_version(&peer_2.wallet_version),
            Some(Ordering::Greater)
        );
        assert_eq!(
            peer_2.compare_version(&peer_3.wallet_version),
            Some(Ordering::Less)
        );

        assert_eq!(
            peer_3.compare_version(&peer_4.wallet_version),
            Some(Ordering::Less)
        );
        assert_eq!(
            peer_4.compare_version(&peer_3.wallet_version),
            Some(Ordering::Greater)
        );

        assert_eq!(
            peer_3.compare_version(&peer_3.wallet_version),
            Some(Ordering::Equal)
        );
        assert_eq!(
            peer_1.compare_version(&peer_1.wallet_version),
            Some(Ordering::Equal)
        );
    }
}

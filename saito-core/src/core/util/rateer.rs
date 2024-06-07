use std::collections::HashMap;

pub struct RateLimiter {
    limits: HashMap<String, usize>,
    windows: HashMap<String, u64>, // Use u64 for duration in milliseconds
    request_counts: HashMap<String, usize>,
    last_request_times: HashMap<String, u64>, // Use u64 for timestamps
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter {
            limits: HashMap::new(),
            windows: HashMap::new(),
            request_counts: HashMap::new(),
            last_request_times: HashMap::new(),
        }
    }

    pub fn set_limit(&mut self, action: &str, limit: usize, window: u64) {
        self.limits.insert(action.to_string(), limit);
        self.windows.insert(action.to_string(), window);
        self.request_counts.insert(action.to_string(), 0);
        self.last_request_times.insert(action.to_string(), 0);
    }

    pub fn can_make_request(&mut self, action: &str, current_time: u64) -> bool {
        let limit = self.limits.get(action).unwrap();
        let window = self.windows.get(action).unwrap();
        let request_count = self.request_counts.get_mut(action).unwrap();
        let last_request_time = self.last_request_times.get_mut(action).unwrap();

        if current_time.saturating_sub(*last_request_time) > *window {
            *request_count = 0;
            *last_request_time = current_time;
        }

        if *request_count < *limit {
            *request_count += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub index: u64,
    pub public_key: Option<SaitoPublicKey>,
    pub block_fetch_url: String,
    pub static_peer_config: Option<util::configuration::PeerConfig>,
    pub challenge_for_peer: Option<SaitoHash>,
    pub key_list: Vec<SaitoPublicKey>,
    pub services: Vec<PeerService>,
    pub last_msg_at: Timestamp,
    pub wallet_version: Version,
    pub core_version: Version,
    pub rate_limiter: RateLimiter, // Add the rate limiter here
}

impl Peer {
    pub fn new(peer_index: u64) -> Peer {
        let mut rate_limiter = RateLimiter::new();
        rate_limiter.set_limit("key_list", 10, 60_000); // Example rate limit for key_list action (60 seconds)
        rate_limiter.set_limit("handshake", 5, 60_000); // Example rate limit for handshake action (60 seconds)

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
            rate_limiter, // Initialize the rate limiter
        }
    }

    // Other methods...

    pub fn can_make_request(&mut self, action: &str, current_time: u64) -> bool {
        self.rate_limiter.can_make_request(action, current_time)
    }
}

impl Network {
    pub async fn handle_received_key_list(
        &mut self,
        peer_index: PeerIndex,
        key_list: Vec<SaitoPublicKey>,
        current_time: u64,
    ) -> Result<(), Error> {
        debug!(
            "handling received key list of length : {:?} from peer : {:?}",
            key_list.len(),
            peer_index
        );

        // Lock peers to read
        let mut peers = self.peer_lock.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);

        if let Some(peer) = peer {
            if !peer.can_make_request("key_list", current_time) {
                warn!("peer {:?} exceeded rate limit for key list", peer_index);
                return Err(Error::new(ErrorKind::Other, "Rate limit exceeded"));
            }

            // Process the key list if the request is allowed
            peer.key_list = key_list;
            Ok(())
        } else {
            error!(
                "peer not found for index : {:?}. cannot handle received key list",
                peer_index
            );
            Err(Error::new(ErrorKind::NotFound, "Peer not found"))
        }
    }

    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet_lock: Arc<RwLock<Wallet>>,
        config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
        current_time: u64,
    ) -> Result<(), Error> {
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if let Some(peer) = peer {
            if !peer.can_make_request("handshake", current_time) {
                warn!("peer {:?} exceeded rate limit for handshake", peer_index);
                return Err(Error::new(ErrorKind::Other, "Rate limit exceeded"));
            }

            peer.handle_handshake_challenge(
                challenge,
                self.io_interface.as_ref(),
                wallet_lock.clone(),
                config_lock,
                current_time,
            )
            .await
        } else {
            error!(
                "peer not found for index : {:?}. cannot handle handshake challenge",
                peer_index
            );
            Err(Error::new(ErrorKind::NotFound, "Peer not found"))
        }
    }
}

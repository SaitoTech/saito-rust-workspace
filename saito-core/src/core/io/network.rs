use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::{debug, error, info, trace, warn};
use tokio::sync::RwLock;

use crate::core::consensus::block::Block;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::peer::Peer;
use crate::core::consensus::peer_collection::PeerCollection;
use crate::core::consensus::transaction::{Transaction, TransactionType};
use crate::core::consensus::wallet::Wallet;
use crate::core::defs::{
    BlockId, PeerIndex, PrintForLog, SaitoHash, SaitoPublicKey, Timestamp,
    PEER_RECONNECT_WAIT_PERIOD,
};
use crate::core::io::interface_io::{InterfaceEvent, InterfaceIO};
use crate::core::msg::block_request::BlockchainRequest;
use crate::core::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::msg::message::Message;
use crate::core::process::keep_time::Timer;
use crate::core::util::configuration::{Configuration, PeerConfig};
use crate::core::util::rate_limiter::RateLimiter;

// #[derive(Debug)]
pub struct Network {
    // TODO : manage peers from network
    pub peer_lock: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    static_peer_configs: Vec<(PeerConfig, Timestamp)>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    pub timer: Timer,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peer_lock: Arc<RwLock<PeerCollection>>,
        wallet_lock: Arc<RwLock<Wallet>>,
        config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
        timer: Timer,
    ) -> Network {
        Network {
            peer_lock,
            io_interface: io_handler,
            static_peer_configs: Default::default(),
            wallet_lock,
            config_lock,
            timer,
        }
    }
    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", block.hash.to_hex());

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node
        if let Some(index) = block.routed_from_peer.as_ref() {
            excluded_peers.push(*index);
        }

        {
            let peers = self.peer_lock.read().await;
            for (index, peer) in peers.index_to_peers.iter() {
                if peer.public_key.is_none() {
                    excluded_peers.push(*index);
                    continue;
                }
            }
        }

        debug!("sending block : {:?} to peers", block.hash.to_hex());
        let message = Message::BlockHeaderHash(block.hash, block.id);
        self.io_interface
            .send_message_to_all(message.serialize().as_slice(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, transaction: &Transaction) {
        // TODO : return if tx is not valid

        let peers = self.peer_lock.read().await;
        let mut wallet = self.wallet_lock.write().await;

        let public_key = wallet.public_key;

        if transaction
            .from
            .first()
            .expect("from slip should exist")
            .public_key
            == public_key
        {
            if let TransactionType::GoldenTicket = transaction.transaction_type {
            } else {
                wallet.add_to_pending(transaction.clone());
            }
        }

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
                .send_message(*index, message.serialize().as_slice())
                .await
                .unwrap();
        }
    }

    pub async fn handle_peer_disconnect(&mut self, peer_index: u64) {
        debug!("handling peer disconnect, peer_index = {}", peer_index);

        self.io_interface
            .disconnect_from_peer(peer_index)
            .await
            .unwrap();

        let mut peers = self.peer_lock.write().await;

        let result = peers.find_peer_by_index(peer_index);

        if result.is_some() {
            let peer = result.unwrap();

            if peer.public_key.is_some() {
                // calling here before removing the peer from collections
                self.io_interface
                    .send_interface_event(InterfaceEvent::PeerConnectionDropped(peer_index));
            }

            let public_key = peer.public_key;
            if peer.static_peer_config.is_some() {
                // This means the connection has been initiated from this side, therefore we must
                // try to re-establish the connection again
                info!(
                    "Static peer disconnected, reconnecting .., Peer ID = {}",
                    peer.index
                );

                // setting to immediately reconnect. if failed, it will connect after a time
                self.static_peer_configs
                    .push((peer.static_peer_config.as_ref().unwrap().clone(), 0));
            } else if peer.public_key.is_some() {
                info!("Peer disconnected, expecting a reconnection from the other side, Peer ID = {}, Public Key = {:?}",
                peer.index, peer.public_key.as_ref().unwrap().to_base58());
            }

            if public_key.is_some() {
                peers.address_to_peers.remove(&public_key.unwrap());
            }
            peers.index_to_peers.remove(&peer_index);
        } else {
            error!("unknown peer : {:?} disconnected", peer_index);
        }
    }
    pub async fn handle_new_peer(&mut self, peer_data: Option<PeerConfig>, peer_index: u64) {
        // TODO : if an incoming peer is same as static peer, handle the scenario;
        debug!("handing new peer : {:?}", &peer_data.unwrap().clone().protocol);
        let protocol = peer_data.unwrap().clone().protocol;
        let mut peers = self.peer_lock.write().await;

        let mut peer = Peer::new(peer_index);
        peer.static_peer_config = peer_data;

        if peer.static_peer_config.is_none() {
            // if we don't have peer data it means this is an incoming connection. so we initiate the handshake
            peer.initiate_handshake(self.io_interface.as_ref())
                .await
                .unwrap();
        } else {
            debug!(
                "removing static peer config : {:?}",
                peer.static_peer_config.as_ref().unwrap()
            );
            let data = peer.static_peer_config.as_ref().unwrap();

            self.static_peer_configs
                .retain(|(config, _reconnect_time)| {
                    config.host != data.host || config.port != data.port
                });
        }

        debug!("new peer added : {:?}", peer_index);
        peers.index_to_peers.insert(peer_index, peer);
        debug!("current peer count = {:?}", peers.index_to_peers.len());
    }
    pub async fn handle_handshake_challenge(
        &self,
        peer_index: u64,
        challenge: HandshakeChallenge,
        wallet_lock: Arc<RwLock<Wallet>>,
        config_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle handshake challenge",
                peer_index
            );
            return;
        }
        let peer = peer.unwrap();
        let current_time = self.timer.get_timestamp_in_ms();

        if !peer.can_make_request("hand", current_time) {
            debug!("peer {:?} exceeded rate limit for key list", peer_index);
            return;
        }else {
            debug!("can make request")
        }


        peer.handle_handshake_challenge(
            challenge,
            self.io_interface.as_ref(),
            wallet_lock.clone(),
            config_lock,
        )
        .await
        .unwrap();
    }
    pub async fn handle_handshake_response(
        &mut self,
        peer_index: u64,
        response: HandshakeResponse,
        wallet_lock: Arc<RwLock<Wallet>>,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        // debug!("handling handshake response");
        let mut peers = self.peer_lock.write().await;

        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            error!(
                "peer not found for index : {:?}. cannot handle handshake response",
                peer_index
            );
            return;
        }
        let peer: &mut Peer = peer.unwrap();
        let result = peer
            .handle_handshake_response(
                response,
                self.io_interface.as_ref(),
                wallet_lock.clone(),
                configs_lock.clone(),
            )
            .await;
        if result.is_err() || peer.public_key.is_none() {
            info!(
                "disconnecting peer : {:?} as handshake response was not handled",
                peer_index
            );
            self.io_interface
                .disconnect_from_peer(peer_index)
                .await
                .unwrap();
            return;
        }
        debug!(
            "peer : {:?} handshake successful for peer : {:?}",
            peer.index,
            peer.public_key.as_ref().unwrap().to_base58()
        );
        self.io_interface
            .send_interface_event(InterfaceEvent::PeerConnected(peer_index));
        let public_key = peer.public_key.unwrap();
        peers.address_to_peers.insert(public_key, peer_index);
        // start block syncing here
        self.request_blockchain_from_peer(peer_index, blockchain_lock.clone())
            .await;
    }
    pub async fn handle_received_key_list(
        &mut self,
        peer_index: PeerIndex,
        key_list: Vec<SaitoPublicKey>,
    ) -> Result<(), Error> {
        debug!(
            "handler received key list of length : {:?} from peer : {:?}",
            key_list.len(),
            peer_index
        );
    
        let current_time = self.timer.get_timestamp_in_ms();
        // Lock peers to write
        let mut peers = self.peer_lock.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
    
        if let Some(peer) = peer {
            // Check rate limit
            if !peer.can_make_request("key_list", current_time) {
                debug!("peer {:?} exceeded rate limit for key list", peer_index);
                return Err(Error::from(ErrorKind::Other));
            }else {
                debug!("can make request")
            }
    
            debug!(
                "handling received keylist of length : {:?} from peer : {:?}",
                key_list.len(),
                peer_index
            );
            peer.key_list = key_list;
            Ok(())
        } else {
            error!(
                "peer not found for index : {:?}. cannot handle received key list",
                peer_index
            );
            Err(Error::from(ErrorKind::NotFound))
        }
    }
    
  
    pub async fn send_key_list(&self, key_list: &[SaitoPublicKey]) {
        debug!("sending key list to all the peers {:?}", key_list);

        self.io_interface
            .send_message_to_all(
                Message::KeyListUpdate(key_list.to_vec())
                    .serialize()
                    .as_slice(),
                vec![],
            )
            .await
            .unwrap();
    }

    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        blockchain_lock: Arc<RwLock<Blockchain>>,
    ) {
        info!("requesting blockchain from peer : {:?}", peer_index);

        let configs = self.config_lock.read().await;
        let blockchain = blockchain_lock.read().await;
        let fork_id = *blockchain.get_fork_id();
        let buffer: Vec<u8>;

        if configs.is_spv_mode() {
            let request;
            {
                if blockchain.last_block_id > blockchain.get_latest_block_id() {
                    debug!(
                        "blockchain request 1 : latest_id: {:?} latest_hash: {:?} fork_id: {:?}",
                        blockchain.last_block_id,
                        blockchain.last_block_hash.to_hex(),
                        fork_id.to_hex()
                    );
                    request = BlockchainRequest {
                        latest_block_id: blockchain.last_block_id,
                        latest_block_hash: blockchain.last_block_hash,
                        fork_id,
                    };
                } else {
                    debug!(
                        "blockchain request 2 : latest_id: {:?} latest_hash: {:?} fork_id: {:?}",
                        blockchain.get_latest_block_id(),
                        blockchain.get_latest_block_hash().to_hex(),
                        fork_id.to_hex()
                    );
                    request = BlockchainRequest {
                        latest_block_id: blockchain.get_latest_block_id(),
                        latest_block_hash: blockchain.get_latest_block_hash(),
                        fork_id,
                    };
                }
            }
            debug!("sending ghost chain request to peer : {:?}", peer_index);
            buffer = Message::GhostChainRequest(
                request.latest_block_id,
                request.latest_block_hash,
                request.fork_id,
            )
            .serialize();
        } else {
            let request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id,
            };
            debug!("sending blockchain request to peer : {:?}", peer_index);
            buffer = Message::BlockchainRequest(request).serialize();
        }
        // need to drop the reference here to avoid deadlocks.
        // We need blockchain lock till here to avoid integrity issues
        drop(blockchain);
        drop(configs);

        self.io_interface
            .send_message(peer_index, buffer.as_slice())
            .await
            .unwrap();
    }
    pub async fn process_incoming_block_hash(
        &mut self,
        block_hash: SaitoHash,
        block_id: BlockId,
        peer_index: PeerIndex,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        mempool_lock: Arc<RwLock<Mempool>>,
    ) -> Option<()> {
        trace!(
            "processing incoming block hash : {:?} from peer : {:?}",
            block_hash.to_hex(),
            peer_index
        );
        let configs = self.config_lock.read().await;
        let block_exists;
        let my_public_key;
        {
            let blockchain = blockchain_lock.read().await;
            let mempool = mempool_lock.read().await;
            let wallet = self.wallet_lock.read().await;

            block_exists = blockchain.is_block_indexed(block_hash)
                || mempool.blocks_queue.iter().any(|b| b.hash == block_hash);
            my_public_key = wallet.public_key;
        }
        if block_exists {
            debug!(
                "block : {:?}-{:?} already exists in chain. not fetching",
                block_id,
                block_hash.to_hex()
            );
            return None;
        }
        let url;
        {
            let peers = self.peer_lock.read().await;
            let wallet = self.wallet_lock.read().await;

            if let Some(peer) = peers.index_to_peers.get(&peer_index) {
                if wallet.wallet_version > peer.wallet_version {
                    warn!(
                    "Not Fetching Block: {:?} from peer :{:?} since peer version is old. expected: {:?} actual {:?} ",
                    block_hash.to_hex(), peer.index, wallet.wallet_version, peer.wallet_version
                );
                    return None;
                }

                if peer.block_fetch_url.is_empty() {
                    debug!(
                        "won't fetch block : {:?} from peer : {:?} since no url found",
                        block_hash.to_hex(),
                        peer_index
                    );
                    return None;
                }
                url = peer.get_block_fetch_url(block_hash, configs.is_spv_mode(), my_public_key);
            } else {
                warn!(
                    "peer : {:?} is not in peer list. cannot generate the block fetch url",
                    peer_index
                );
                return None;
            }
        }

        debug!(
            "fetching block for incoming hash : {:?}-{:?}",
            block_id,
            block_hash.to_hex()
        );

        if self
            .io_interface
            .fetch_block_from_peer(block_hash, peer_index, url.as_str(), block_id)
            .await
            .is_err()
        {
            // failed fetching block from peer
            warn!(
                "failed fetching block : {:?} for block hash. so unmarking block as fetching",
                block_hash.to_hex()
            );
        }
        Some(())
    }

    pub async fn initialize_static_peers(
        &mut self,
        configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    ) {
        let configs = configs_lock.read().await;

        configs
            .get_peer_configs()
            .clone()
            .drain(..)
            .for_each(|config| {
                self.static_peer_configs.push((config, 0));
            });
        if !self.static_peer_configs.is_empty() {
            self.static_peer_configs.get_mut(0).unwrap().0.is_main = true;
        }
        trace!("static peers : {:?}", self.static_peer_configs);
    }

    pub async fn connect_to_static_peers(&mut self) {
        let current_time = self.timer.get_timestamp_in_ms();
        for (peer, reconnect_after) in &mut self.static_peer_configs {
            trace!("connecting to peer : {:?}", peer);
            if *reconnect_after > current_time {
                continue;
            }
            *reconnect_after = current_time + PEER_RECONNECT_WAIT_PERIOD;
            self.io_interface
                .connect_to_peer(peer.clone())
                .await
                .unwrap();
        }
    }

    pub async fn send_pings(&mut self) {
        let current_time = self.timer.get_timestamp_in_ms();
        let mut peers = self.peer_lock.write().await;
        for (_, peer) in peers.index_to_peers.iter_mut() {
            peer.send_ping(current_time, self.io_interface.as_ref())
                .await;
        }
    }

    // pub async fn check_rate_limit(&mut self, peer_index: u64) -> bool {
    //     let current_time = self.timer.get_timestamp_in_ms();
    //     {
    //         let mut rate_limiter = self.rate_limiter.write().await;
    //         if !rate_limiter.can_process_more(peer_index, current_time) {
    //             error!("peer {:?} exceeded rate limit for key list", peer_index);
    //             return false;
    //         } else {
    //             return true;
    //         }
    //     }

    //     // self.rate_limiter.can_process_more(peer_index, current_time)
    // }

    pub async fn update_peer_timer(&mut self, peer_index: PeerIndex) {
        let mut peers = self.peer_lock.write().await;
        let peer = peers.index_to_peers.get_mut(&peer_index);
        if peer.is_none() {
            return;
        }
        let peer = peer.unwrap();
        peer.last_msg_at = self.timer.get_timestamp_in_ms();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ util::test::test_manager};
    use rand::Rng;


    #[tokio::test]
    async fn test_keylist_rate_limiter() {
        let mut t1 = test_manager::test::TestManager::default();
        let TOKENS: usize = 10;
        let peer2_index: u64 = 0;
        let mut peer2;
    
        {
            let mut peers = t1.network.peer_lock.write().await;

          peer2  = Peer::new(peer2_index);
      
    
            // peer2.rate_limiter.set_limit("key_list", 10, 60_000); 
    
            let peer_data = PeerConfig {
                host: String::from(""),
                port: 8080,
                protocol: String::from(""),
                synctype: String::from(""),
                is_main: true,
            };
    
            peer2.static_peer_config = Some(peer_data);
            peers.index_to_peers.insert(peer2_index, peer2.clone());
            println!("Current peer count = {:?}", peers.index_to_peers.len());
        }
    
        for i in 0..40 {
        
            let key_list: Vec<SaitoPublicKey> = (0..11)
                .map(|_| {
                    let mut key = [0u8; 33];
                    rand::thread_rng().fill(&mut key[..]);
                    key
                })
                .collect();
    
       
            let result = t1
                .network
                .handle_received_key_list(peer2_index, key_list)
                .await;
    
            dbg!(&result);
    
            if i < TOKENS {

                assert!(result.is_ok(), "Expected Ok, got {:?}", result);
            } else {
                assert!(result.is_err(), "Expected Err, got {:?}", result);
            }
        }
        dbg!(peer2);
    }
    
    // #[tokio::test]

    // async fn test_rate_limter_token_refill() {
    //     let TOKENS: u64 = 10;
    //     let peer_index: u64 = 1;
    //     let initial_time: u64 = 100000;
    //     let refill_time: u64 = initial_time + 60000; // 1 minute later

    //     let mut rate_limiter = RateLimiter::default(TOKENS);

    //     // Consume all tokens
    //     for _ in 0..TOKENS {
    //         assert!(rate_limiter.can_process_more(peer_index, initial_time));
    //     }

    //     assert!(!rate_limiter.can_process_more(peer_index, initial_time));

    //     // Simulate the passage of time and refill tokens
    //     assert!(rate_limiter.can_process_more(peer_index, refill_time));
    // }
}

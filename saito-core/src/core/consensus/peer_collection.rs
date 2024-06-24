use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::consensus::peer::Peer;
use crate::core::defs::{PeerIndex, SaitoPublicKey};

#[derive(Clone, Debug)]
pub struct PeerCounter {
    counter: PeerIndex,
}
impl Default for PeerCounter {
    fn default() -> Self {
        PeerCounter { counter: 0 }
    }
}

impl PeerCounter {
    pub fn get_next_index(&mut self) -> PeerIndex {
        self.counter += 1;
        self.counter
    }
}
#[derive(Debug, Clone)]
pub struct PeerCollection {
    pub index_to_peers: HashMap<PeerIndex, Peer>,
    pub address_to_peers: HashMap<SaitoPublicKey, PeerIndex>,
    pub peer_counter: PeerCounter,
}

impl PeerCollection {
    pub fn new() -> PeerCollection {
        PeerCollection {
            index_to_peers: Default::default(),
            address_to_peers: Default::default(),
            peer_counter: Default::default(),
        }
    }

    pub fn find_peer_by_address(&self, address: &SaitoPublicKey) -> Option<&Peer> {
        let result = self.address_to_peers.get(address);
        if result.is_none() {
            return None;
        }

        return self.find_peer_by_index(*result.unwrap());
    }

    pub fn find_peer_by_index(&self, peer_index: u64) -> Option<&Peer> {
        self.index_to_peers.get(&peer_index)
    }
    pub fn find_peer_by_index_mut(&mut self, peer_index: u64) -> Option<&mut Peer> {
        self.index_to_peers.get_mut(&peer_index)
    }

    pub fn remove_peer(&mut self, peer_index: PeerIndex) -> Option<Peer> {
        let peer = self.index_to_peers.remove(&peer_index);
        if let Some(peer) = peer.as_ref() {
            if let Some(key) = peer.get_public_key() {
                self.address_to_peers.remove(&key);
            }
        }
        peer
    }
}

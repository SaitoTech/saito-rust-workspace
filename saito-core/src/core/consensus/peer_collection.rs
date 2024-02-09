use std::collections::HashMap;

use crate::common::defs::SaitoPublicKey;
use crate::core::consensus::peer::Peer;

#[derive(Debug, Clone)]
pub struct PeerCollection {
    pub index_to_peers: HashMap<u64, Peer>,
    pub address_to_peers: HashMap<SaitoPublicKey, u64>,
}

impl PeerCollection {
    pub fn new() -> PeerCollection {
        PeerCollection {
            index_to_peers: Default::default(),
            address_to_peers: Default::default(),
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
}

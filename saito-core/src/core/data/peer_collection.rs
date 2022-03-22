use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::common::defs::SaitoPublicKey;
use crate::core::data::peer::Peer;

pub struct PeerCollection {
    pub index_to_peers: HashMap<u64, Arc<Peer>>,
    pub address_to_peers: HashMap<SaitoPublicKey, Arc<Peer>>,
}

impl PeerCollection {
    pub fn new() -> PeerCollection {
        PeerCollection {
            index_to_peers: Default::default(),
            address_to_peers: Default::default(),
        }
    }
    pub fn add_peer(&mut self, peer: Peer) {
        let peer = Arc::new(peer);
        // TODO : check if the keys are already in use
        self.index_to_peers.insert(peer.peer_index, peer.clone());
        self.address_to_peers
            .insert(peer.peer_address, peer.clone());
    }
    pub fn remove_peer_by_index(&mut self, index: u64) {
        let peer = self.index_to_peers.remove(&index);
        if peer.is_some() {
            self.address_to_peers.remove(&peer.unwrap().peer_address);
        }
    }
    pub fn remove_peer_by_address(&mut self, address: &SaitoPublicKey) {
        let peer = self
            .address_to_peers
            .remove::<SaitoPublicKey>(address.borrow());
        if peer.is_some() {
            self.index_to_peers.remove(&peer.unwrap().peer_index);
        }
    }
    pub fn find_peer_by_index(&self, index: u64) -> Option<Arc<Peer>> {
        let result = self.index_to_peers.get(&index);
        if result.is_some() {
            let peer = result.unwrap();
            let peer = peer.clone();
            return Some(peer);
        }
        None
    }
    pub fn find_peer_by_address(&self, address: SaitoPublicKey) -> Option<Arc<Peer>> {
        let result = self.address_to_peers.get(&address);
        if result.is_some() {
            let peer = result.unwrap().clone();
            return Some(peer);
        }
        None
    }
}

use std::collections::HashMap;

use crate::common::defs::SaitoPublicKey;
use crate::core::data::peer::Peer;

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
}

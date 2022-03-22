use crate::common::defs::SaitoPublicKey;

pub struct Peer {
    pub peer_index: u64,
    pub peer_address: SaitoPublicKey,
}

impl Peer {
    pub fn new(peer_index: u64, address: SaitoPublicKey) -> Peer {
        Peer {
            peer_index,
            peer_address: address,
        }
    }
}

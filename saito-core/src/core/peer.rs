pub struct Peer {
    peer_index: u64,
}

impl Peer {
    pub fn new(peer_index: u64) -> Peer {
        Peer { peer_index }
    }
}

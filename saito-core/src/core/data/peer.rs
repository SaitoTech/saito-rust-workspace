use crate::common::defs::SaitoPublicKey;
use crate::common::handle_io::HandleIo;
use crate::core::data;

pub struct Peer {
    pub peer_index: u64,
    pub peer_address: SaitoPublicKey,
    pub static_peer_config: Option<data::configuration::Peer>,
}

impl Peer {
    pub fn new(peer_index: u64) -> Peer {
        Peer {
            peer_index,
            peer_address: [0; 33],
            static_peer_config: None,
        }
    }
    pub async fn initiate_handshake(&mut self, io_handler: &Box<dyn HandleIo + Send + Sync>) {
        // TODO : implement this
    }
}

use log::debug;

use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoSignature};
use crate::common::handle_io::HandleIo;
use crate::core::data;

#[derive(Debug, Clone)]
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
        debug!("initiating handshake : {:?}", self.peer_index);
        // TODO : implement this
        todo!()
    }
    pub async fn handle_handshake_challenge(
        &mut self,
        io_handler: &Box<dyn HandleIo + Send + Sync>,
    ) {
    }
    pub fn get_block_fetch_url(&self, block_hash: SaitoHash) -> String {
        let config = self.static_peer_config.as_ref().unwrap();
        format!(
            "{:?}://{:?}:{:?}/block/{:?}",
            config.protocol,
            config.host,
            config.port,
            hex::encode(block_hash)
        )
    }
}

use crate::common::defs::SaitoHash;
use crate::core::data;

#[derive(Debug)]
pub enum NetworkEvent {
    OutgoingNetworkMessage {
        peer_index: u64,
        buffer: Vec<u8>,
    },
    BlockFetchRequest {
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        request_id: u64,
    },
    BlockFetched {
        block_hash: SaitoHash,
        peer_index: u64,
        buffer: Vec<u8>,
        request_id: u64,
    },
    PeerDisconnected {
        peer_index: u64,
    },
}

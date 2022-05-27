use crate::common::defs::SaitoHash;
use crate::core::data;

#[derive(Debug)]
pub enum NetworkEvent {
    OutgoingNetworkMessage {
        peer_index: u64,
        buffer: Vec<u8>,
    },
    OutgoingNetworkMessageForAll {
        buffer: Vec<u8>,
        exceptions: Vec<u64>,
    },
    IncomingNetworkMessage {
        peer_index: u64,
        buffer: Vec<u8>,
    },
    ConnectToPeer {
        peer_details: data::configuration::Peer,
    },
    PeerConnectionResult {
        peer_details: Option<data::configuration::Peer>,
        result: Result<u64, std::io::Error>,
    },
    PeerDisconnected {
        peer_index: u64,
    },
    BlockFetchRequest {
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
    },
    BlockFetched {
        block_hash: SaitoHash,
        peer_index: u64,
        buffer: Vec<u8>,
    },
}

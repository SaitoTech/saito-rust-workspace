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
        peer_details: data::configuration::PeerConfig,
    },
    DisconnectFromPeer {
        peer_index: u64,
    },
    PeerConnectionResult {
        peer_details: Option<data::configuration::PeerConfig>,
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
    BlockFetchFailed {
        block_hash: SaitoHash,
    },
}

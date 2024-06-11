use crate::core::defs::{BlockId, PeerIndex, SaitoHash};
use crate::core::io::network::PeerDisconnectType;
use crate::core::util;

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
        peer_details: util::configuration::PeerConfig,
    },
    DisconnectFromPeer {
        peer_index: u64,
    },
    PeerConnectionResult {
        peer_details: Option<util::configuration::PeerConfig>,
        result: Result<u64, std::io::Error>,
    },
    PeerDisconnected {
        peer_index: u64,
        disconnect_type: PeerDisconnectType,
    },
    BlockFetchRequest {
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        block_id: BlockId,
    },
    BlockFetched {
        block_hash: SaitoHash,
        block_id: BlockId,
        peer_index: PeerIndex,
        buffer: Vec<u8>,
    },
    BlockFetchFailed {
        block_hash: SaitoHash,
        peer_index: u64,
        block_id: BlockId,
    },
}

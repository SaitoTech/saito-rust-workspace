use crate::common::defs::SaitoHash;
use crate::core::data;
use crate::core::data::block::Block;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::transaction::Transaction;

#[derive(Clone, Debug)]
pub enum GlobalEvent {
    // broadcast when a block is received but parent is unknown
    MissingBlock { peer_id: SaitoHash, hash: SaitoHash },
    // broadcast when the longest chain block changes
    BlockchainNewLongestChainBlock { hash: SaitoHash, difficulty: u64 },
    // broadcast when a block is successfully added
    BlockchainAddBlockSuccess { hash: SaitoHash },
    // broadcast when a block is unsuccessful at being added
    BlockchainAddBlockFailure { hash: SaitoHash },
    // broadcast when the miner finds a golden ticket
    MinerNewGoldenTicket { ticket: GoldenTicket },
    // broadcast when the blockchain wants to broadcast a block to peers
    BlockchainSavedBlock { hash: SaitoHash },
    // handle transactions which we've created "ourself" - interact with saitocli
    WalletNewTransaction { transaction: Transaction },
}

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

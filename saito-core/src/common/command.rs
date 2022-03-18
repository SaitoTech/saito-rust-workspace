use std::io::Error;

use crate::common::defs::SaitoHash;
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

pub enum InterfaceEvent {
    OutgoingNetworkMessage(u64, Vec<u8>),
    IncomingNetworkMessage(u64, Vec<u8>),
    DataSaveRequest(String, Vec<u8>),
    // can do without this.
    DataSaveResponse(String, Error),
    DataReadRequest(String),
    DataReadResponse(String, Vec<u8>, Error),
    ConnectToPeer(String),
    PeerConnected(u64, String),
    PeerDisconnected(String),
}

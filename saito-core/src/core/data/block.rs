use crate::common::defs::{SaitoHash, SaitoPublicKey, Signature};

pub struct Block {
    id: u64,
    timestamp: u64,
    previous_block_hash: SaitoHash,
    creator: SaitoPublicKey,
    merkle_root: SaitoHash,
    signature: Signature,
    treasury: u64,
    burnfee: u64,
    difficulty: u64,
    staking_treasury: u64,
}

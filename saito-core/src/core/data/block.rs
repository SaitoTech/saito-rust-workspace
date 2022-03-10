use crate::common::defs::{Hash32, PublicKey, Signature};

pub struct Block {
    id: u64,
    timestamp: u64,
    previous_block_hash: Hash32,
    creator: PublicKey,
    merkle_root: Hash32,
    signature: Signature,
    treasury: u64,
    burnfee: u64,
    difficulty: u64,
    staking_treasury: u64,
}

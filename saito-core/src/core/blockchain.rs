use std::sync::{Arc, RwLock};

use crate::common::defs::Hash32;
use crate::core::blockring::BlockRing;
use crate::core::wallet::Wallet;

pub struct Blockchain {
    pub block_ring: BlockRing,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    genesis_block_id: u64,
    fork_id: Hash32,
}

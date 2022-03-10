use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::common::defs::Hash32;
use crate::core::blockring::BlockRing;
use crate::core::data::block::Block;
use crate::core::staking::Staking;
use crate::core::utxo_set::UtxoSet;
use crate::core::wallet::Wallet;

pub struct Blockchain {
    pub staking: Staking,
    pub utxoset: UtxoSet,
    pub block_ring: BlockRing,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub blocks: HashMap<Hash32, Block>,
    genesis_block_id: u64,
    fork_id: Hash32,
}

impl Blockchain {}

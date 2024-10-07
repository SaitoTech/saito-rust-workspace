use crate::core::consensus::block::Block;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::defs::{BlockHash, BlockId, ForkId, Timestamp};
use ahash::HashMap;
use std::time::Duration;

pub const FORK_BLOCK_DEPTH_LIMIT: BlockId = 1_000;

pub(crate) enum BlockDepthCheckResult {
    FetchBlock(BlockId, BlockHash),
    ProceedNormally(Block),
}

#[derive(Default, Debug, Clone)]
pub struct BlockDepthLimitChecker {
    count: u64,
    window: Timestamp,
    block_checks: HashMap<BlockId, Block>,
}

impl BlockDepthLimitChecker {
    pub fn builder(count: u64, duration: Duration) -> Self {
        Self {
            count,
            window: duration.as_millis() as Timestamp,
            block_checks: Default::default(),
        }
    }
    pub fn should_fetch_fork_first(
        &mut self,
        block: Block,
        blockchain: &Blockchain,
    ) -> BlockDepthCheckResult {
        if block.id > blockchain.get_latest_block_id() + FORK_BLOCK_DEPTH_LIMIT {
            return BlockDepthCheckResult::FetchBlock(block.id, block.hash);
        }
        todo!()
    }
}

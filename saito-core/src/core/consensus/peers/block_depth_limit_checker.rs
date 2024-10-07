use crate::core::defs::{BlockId, Timestamp};
use std::time::Duration;

#[derive(Default, Debug, Copy, Clone)]
pub struct BlockDepthLimitChecker {
    count: u64,
    window: Timestamp,
}

impl BlockDepthLimitChecker {
    pub fn increase_block_count_at_depth(&mut self, block_depth: BlockId) {
        todo!()
    }
    pub fn has_limit_exceeded(&mut self, current_time: Timestamp) -> bool {
        false
    }

    pub fn builder(count: u64, duration: Duration) -> Self {
        Self {
            count,
            window: duration.as_millis() as Timestamp,
        }
    }
}

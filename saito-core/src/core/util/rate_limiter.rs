use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RateLimiter {
    limits: HashMap<u64, TokenBucket>,
    max_tokens: u64,
}

impl RateLimiter {
    pub fn default(max_tokens: u64) -> Self {
        Self {
            limits: HashMap::new(),
            max_tokens,
        }
    }

    pub fn can_process_more(&mut self, peer_index: u64, current_time: u64) -> bool {
        let max_tokens = self.max_tokens;
        let entry = self
            .limits
            .entry(peer_index)
            .or_insert_with(|| TokenBucket::default(max_tokens, current_time));
        entry.consume_token_if_available(current_time)
    }
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    tokens: u64,
    max_tokens: u64,
    last_refill: u64,
}

impl TokenBucket {
    pub fn default(max_tokens: u64, current_time: u64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            last_refill: current_time,
        }
    }

    pub fn consume_token_if_available(&mut self, current_time: u64) -> bool {
        let elapsed = current_time - self.last_refill;

        if elapsed >= 60000 {
            self.tokens = self.max_tokens;
            self.last_refill = current_time;
        }

        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}

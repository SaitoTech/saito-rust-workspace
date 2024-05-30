use std::collections::HashMap;
use tokio::time::{self, Duration, Instant};

#[derive(Debug, Clone)]
pub struct RateLimiter {
    limits: HashMap<u64, TokenBucket>,
    tokens: u64,
}

impl RateLimiter {
    pub fn default(tokens: u64) -> Self {
        Self {
            limits: HashMap::new(),
            tokens,
        }
    }

    pub fn can_process_more(&mut self, peer_index: u64) -> bool {
        let tokens = self.tokens;
        let now = Instant::now();
        let entry = self
            .limits
            .entry(peer_index)
            .or_insert_with(|| TokenBucket::default(tokens, Instant::now()));
        entry.consume_token_if_available(now)
    }
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    tokens: u64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn default(tokens: u64, now: Instant) -> Self {
        Self {
            tokens,
            last_refill: now,
        }
    }

    pub fn consume_token_if_available(&mut self, current_time: Instant) -> bool {
        let elapsed = current_time.duration_since(self.last_refill);

        if elapsed >= Duration::from_secs(60) {
            self.tokens = self.tokens;
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

use std::collections::HashMap;
use tokio::time::{self, Duration, Instant};

#[derive(Debug, Clone)]
pub struct RateLimiter {
    limits: HashMap<u64, TokenBucket>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limits: HashMap::new(),
        }
    }

    pub fn check(&mut self, peer_index: u64) -> bool {
        let entry = self
            .limits
            .entry(peer_index)
            .or_insert_with(TokenBucket::new);
        entry.try_consume()
    }
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    tokens: usize,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new() -> Self {
        Self {
            tokens: 20,
            last_refill: Instant::now(),
        }
    }

    pub fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        if elapsed >= Duration::from_secs(60) {
            self.tokens = 10;
            self.last_refill = now;
        }

        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}

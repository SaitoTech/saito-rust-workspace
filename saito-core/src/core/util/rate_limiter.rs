use crate::core::defs::Timestamp;

pub enum RateLimiterRequestType {
    KeyList,
    HandshakeChallenge,
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    limit: usize,                 // Max allowed requests in the window
    window: Timestamp,            // Window duration in milliseconds
    request_count: usize,         // Number of requests made in the current window
    last_request_time: Timestamp, // Timestamp of the last request in milliseconds
}

impl RateLimiter {
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    pub fn set_window(&mut self, window: u64) {
        self.window = window;
    }

    pub fn can_make_request(&mut self, current_time: u64) -> bool {
        if current_time.saturating_sub(self.last_request_time) > self.window {
            self.request_count = 0;
            self.last_request_time = current_time;
        }

        if self.request_count < self.limit {
            self.request_count += 1;
            true
        } else {
            false
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        RateLimiter {
            limit: 10,      // Default limit of 10 requests
            window: 60_000, // Default window of 60 seconds
            request_count: 0,
            last_request_time: 0,
        }
    }
}

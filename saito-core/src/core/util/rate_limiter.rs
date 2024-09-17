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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rate_limiter_test() {
        let rate_limiter = RateLimiter::default();

        assert_eq!(rate_limiter.limit, 10);
        assert_eq!(rate_limiter.window, 60_000);
        assert_eq!(rate_limiter.request_count, 0);
        assert_eq!(rate_limiter.last_request_time, 0);
    }

    #[test]
    fn set_limit_test() {
        let mut rate_limiter = RateLimiter::default();
        rate_limiter.set_limit(5);

        assert_eq!(rate_limiter.limit, 5);
    }

    #[test]
    fn set_window_test() {
        let mut rate_limiter = RateLimiter::default();
        rate_limiter.set_window(120_000); // 120 seconds

        assert_eq!(rate_limiter.window, 120_000);
    }

    #[test]
    fn can_make_request_within_limit_test() {
        let mut rate_limiter = RateLimiter::default();
        let current_time = 10_000;
        assert!(rate_limiter.can_make_request(current_time));
        assert_eq!(rate_limiter.request_count, 1);
    }

    #[test]
    fn can_make_request_exceeding_limit_test() {
        let mut rate_limiter = RateLimiter::default();
        let current_time = 10_000;
        for _ in 0..rate_limiter.limit {
            assert!(rate_limiter.can_make_request(current_time));
        }
        assert!(!rate_limiter.can_make_request(current_time));
        assert_eq!(rate_limiter.request_count, rate_limiter.limit);
    }

    #[test]
    fn can_make_request_after_window_reset_test() {
        let mut rate_limiter = RateLimiter::default();
        let current_time = 10_000;

        for _ in 0..rate_limiter.limit {
            assert!(rate_limiter.can_make_request(current_time));
        }
        let new_time = current_time + rate_limiter.window + 1;
        assert!(rate_limiter.can_make_request(new_time));
        assert_eq!(rate_limiter.request_count, 1);
        assert_eq!(rate_limiter.last_request_time, new_time);
    }

    #[test]
    fn can_make_request_reset_on_new_time_test() {
        let mut rate_limiter = RateLimiter::default();
        let initial_time = 10_000;

        for _ in 0..rate_limiter.limit {
            assert!(rate_limiter.can_make_request(initial_time));
        }

        let later_time = initial_time + rate_limiter.window + 500;
        assert!(rate_limiter.can_make_request(later_time));
        dbg!("{}", &rate_limiter);
        assert_eq!(rate_limiter.request_count, 1);
        assert_eq!(rate_limiter.last_request_time, later_time);
    }
}

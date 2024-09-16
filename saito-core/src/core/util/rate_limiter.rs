use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RateLimiter {
    limits: HashMap<String, usize>,
    windows: HashMap<String, u64>, // Use u64 for duration in milliseconds
    request_counts: HashMap<String, usize>,
    last_request_times: HashMap<String, u64>, // Use u64 for timestamps
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter {
            limits: HashMap::new(),
            windows: HashMap::new(),
            request_counts: HashMap::new(),
            last_request_times: HashMap::new(),
        }
    }

    pub fn set_limit(&mut self, action: &str, limit: usize, window: u64) {
        self.limits.insert(action.to_string(), limit);
        self.windows.insert(action.to_string(), window);
        self.request_counts.insert(action.to_string(), 0);
        self.last_request_times.insert(action.to_string(), 0);
    }

    pub fn can_make_request(&mut self, action: &str, current_time: u64) -> bool {
        let limit = self.limits.get(action).unwrap();
        let window = self.windows.get(action).unwrap();
        let request_count = self.request_counts.get_mut(action).unwrap();
        let last_request_time = self.last_request_times.get_mut(action).unwrap();

        if current_time.saturating_sub(*last_request_time) > *window {
            *request_count = 0;
            *last_request_time = current_time;
        }

        if *request_count < *limit {
            *request_count += 1;
            true
        } else {
            false
        }
    }
}

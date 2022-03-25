use std::time::{SystemTime, UNIX_EPOCH};

use saito_core::common::keep_time::KeepTime;

pub struct TimeKeeper {}

impl KeepTime for TimeKeeper {
    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

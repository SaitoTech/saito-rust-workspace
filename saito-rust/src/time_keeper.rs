use saito_core::core::defs::Timestamp;
use std::time::{SystemTime, UNIX_EPOCH};

use saito_core::core::process::keep_time::KeepTime;

#[derive(Clone)]
pub struct TimeKeeper {}

impl KeepTime for TimeKeeper {
    fn get_timestamp_in_ms(&self) -> Timestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as Timestamp
    }
}

// #[derive(Clone)]
// pub struct HastenedTimeKeeper {
//     start_time: Timestamp,
//     time_multiplier: u64,
// }
//
// impl HastenedTimeKeeper {
//     pub fn new(start_time: Timestamp, time_multiplier: u64) -> HastenedTimeKeeper {
//         HastenedTimeKeeper {
//             start_time,
//             time_multiplier,
//         }
//     }
// }
//
// impl KeepTime for HastenedTimeKeeper {
//     fn get_timestamp_in_ms(&self) -> Timestamp {
//         let current_time = SystemTime::now()
//             .duration_since(UNIX_EPOCH)
//             .unwrap()
//             .as_millis() as Timestamp;
//
//         let time_since_start = current_time - self.start_time;
//         self.start_time + time_since_start * self.time_multiplier
//     }
// }

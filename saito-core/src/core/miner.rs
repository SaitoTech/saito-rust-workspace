use std::time::{Duration, Instant};

use log::{debug, info};

use crate::common::run_task::RunTask;

pub struct Miner {
    time_to_next_tick: u128,
}

impl Miner {
    pub fn new() -> Miner {
        Miner {
            time_to_next_tick: 0,
        }
    }
    pub fn init(&mut self, task_runner: &dyn RunTask) {
        debug!("Miner.init");
    }
    pub fn mine(&mut self, duration: Duration) {
        self.time_to_next_tick = self.time_to_next_tick + duration.as_micros();
        if self.time_to_next_tick >= 5_000_000 {
            self.time_to_next_tick = 0;
        }

        if self.time_to_next_tick > 0 {
            return;
        }

        info!("block created");
    }
}

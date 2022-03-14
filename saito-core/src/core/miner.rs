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
        debug!("main thread id = {:?}", std::thread::current().id());
        task_runner.run(Box::pin(move || {
            let mut last_time = Instant::now();
            let mut counter = 0;
            debug!("miner thread id = {:?}", std::thread::current().id());
            loop {
                let current_time = Instant::now();
                let duration = current_time.duration_since(last_time);

                if duration.as_micros() > 1_000_000 {
                    info!("counter : {:?}", counter);
                    last_time = current_time;
                    counter = counter + 1;
                }
                if counter < 5 {
                    continue;
                }
                info!("block created");

                counter = 0;
            }
        }));
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

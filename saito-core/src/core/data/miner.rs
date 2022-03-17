use std::io::Error;
use std::time::{Duration, Instant};

use log::{debug, info, trace};

use crate::common::command::{InterfaceEvent, SaitoEvent};
use crate::common::process_event::ProcessEvent;
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
    pub fn init(&mut self, task_runner: &dyn RunTask) -> Result<(), Error> {
        debug!("Miner.init");
        Ok(())
    }
    pub fn on_timer(&mut self, duration: Duration) -> Option<()> {
        trace!("Miner.on_timer");
        None
    }
    pub fn mine(&mut self, duration: Duration) {
        trace!("Miner.mine");
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

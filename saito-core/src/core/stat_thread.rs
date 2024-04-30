use std::collections::VecDeque;
use std::time::Duration;

use async_trait::async_trait;

use crate::core::defs::Timestamp;
use crate::core::io::interface_io::InterfaceIO;
use crate::core::io::network_event::NetworkEvent;
use crate::core::process::process_event::ProcessEvent;

pub struct StatThread {
    // pub file: File,
    pub stat_queue: VecDeque<String>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
}

impl StatThread {
    pub async fn new(io_interface: Box<dyn InterfaceIO + Send + Sync>) -> StatThread {
        StatThread {
            io_interface,
            stat_queue: VecDeque::new(),
        }
    }
}

#[async_trait]
impl ProcessEvent<String> for StatThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        None
    }

    async fn process_timer_event(&mut self, _duration: Duration) -> Option<()> {
        let mut work_done = false;

        for stat in self.stat_queue.drain(..) {
            let stat = stat + "\r\n";
            self.io_interface
                .write_value("./data/saito.stats", stat.as_bytes(), true)
                .await
                .unwrap();
            work_done = true;
        }
        if work_done {
            self.io_interface
                .flush_data("./data/saito.stats")
                .await
                .unwrap();
            return Some(());
        }
        None
    }

    async fn process_event(&mut self, event: String) -> Option<()> {
        self.stat_queue.push_back(event);
        return Some(());
    }

    async fn on_init(&mut self) {}

    async fn on_stat_interval(&mut self, _current_time: Timestamp) {}
}

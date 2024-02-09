use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use saito_core::core::defs::Timestamp;
use saito_core::core::io::network_event::NetworkEvent;
use saito_core::core::process::process_event::ProcessEvent;

pub struct StatThread {
    pub file: File,
    pub stat_queue: VecDeque<String>,
}

impl StatThread {
    pub async fn new() -> StatThread {
        let path = Path::new("./data/saito.stats");

        let file = File::create(path).await.unwrap();

        StatThread {
            file,
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
            self.file.write_all(stat.as_bytes()).await.unwrap();
            work_done = true;
        }
        if work_done {
            self.file.flush().await.expect("stat file flush failed");
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

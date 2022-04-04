use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::defs::SaitoHash;
use crate::common::handle_io::HandleIo;
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::miner::Miner;
use crate::core::mempool_controller::MempoolEvent;

pub enum MinerEvent {
    Mine { hash: SaitoHash, difficulty: u64 },
}

pub struct MinerController {
    pub miner: Arc<RwLock<Miner>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
}

impl MinerController {}

#[async_trait]
impl ProcessEvent<MinerEvent> for MinerController {
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        debug!("processing new interface event");

        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        trace!("processing timer event : {:?}", duration.as_micros());
        None
    }

    async fn process_event(&mut self, event: MinerEvent) -> Option<()> {
        match event {
            MinerEvent::Mine { hash, difficulty } => {
                let miner = self.miner.read().await;
                miner
                    .mine(hash, difficulty, self.sender_to_mempool.clone())
                    .await;
            }
        }
        None
    }

    async fn on_init(&mut self) {}
}

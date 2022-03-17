use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::common::command::{InterfaceEvent, SaitoEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::data::miner::Miner;

pub enum MinerEvent {}

pub struct MinerController {
    pub miner: Arc<RwLock<Miner>>,
}

impl MinerController {}

impl ProcessEvent<MinerEvent> for MinerController {
    fn process_saito_event(&mut self, event: SaitoEvent) -> Option<()> {
        todo!()
    }

    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        todo!()
    }

    fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        todo!()
    }

    fn process_event(&mut self, event: MinerEvent) -> Option<()> {
        todo!()
    }
}

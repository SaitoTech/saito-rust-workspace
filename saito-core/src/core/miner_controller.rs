use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{InterfaceEvent, SaitoEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::miner::Miner;
use crate::core::mempool_controller::MempoolEvent;

pub enum MinerEvent {}

pub struct MinerController {
    pub miner: Arc<RwLock<Miner>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub io_handler: Box<dyn HandleIo + Send>,
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

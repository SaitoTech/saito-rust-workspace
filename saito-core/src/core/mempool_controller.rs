use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{InterfaceEvent, SaitoEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::mempool::Mempool;
use crate::core::miner_controller::MinerEvent;

pub enum MempoolEvent {}

pub struct MempoolController {
    pub mempool: Arc<RwLock<Mempool>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub io_handler: Box<dyn HandleIo + Send>,
}

impl MempoolController {}

impl ProcessEvent<MempoolEvent> for MempoolController {
    fn process_saito_event(&mut self, event: SaitoEvent) -> Option<()> {
        todo!()
    }

    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        todo!()
    }

    fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        todo!()
    }

    fn process_event(&mut self, event: MempoolEvent) -> Option<()> {
        todo!()
    }
}

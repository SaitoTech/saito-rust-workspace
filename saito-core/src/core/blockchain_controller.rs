use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{InterfaceEvent, SaitoEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::data::blockchain::Blockchain;
use crate::core::mempool_controller::MempoolEvent;
use crate::core::miner_controller::MinerEvent;

pub enum BlockchainEvent {}

pub struct BlockchainController {
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_mempool: Sender<MempoolEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub io_handler: Box<dyn HandleIo + Send>,
}

impl BlockchainController {}

impl ProcessEvent<BlockchainEvent> for BlockchainController {
    fn process_saito_event(&mut self, event: SaitoEvent) -> Option<()> {
        todo!()
    }

    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        todo!()
    }

    fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        todo!()
    }

    fn process_event(&mut self, event: BlockchainEvent) -> Option<()> {
        todo!()
    }
}

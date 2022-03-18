use std::sync::Arc;
use std::time::Duration;

use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
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
    fn process_global_event(&mut self, event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        trace!("processing new interface event");

        None
    }

    fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        trace!("processing timer event : {:?}", duration.as_micros());
        None
    }

    fn process_event(&mut self, event: BlockchainEvent) -> Option<()> {
        trace!("processing blockchain event");
        None
    }
}

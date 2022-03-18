use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::trace;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::handle_io::HandleIo;
use crate::common::process_event::ProcessEvent;
use crate::core::blockchain_controller::BlockchainEvent;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::mempool::Mempool;
use crate::core::miner_controller::MinerEvent;

pub enum MempoolEvent {}

pub struct MempoolController {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub sender_to_blockchain: Sender<BlockchainEvent>,
    pub sender_to_miner: Sender<MinerEvent>,
    pub io_handler: Box<dyn HandleIo + Send>,
}

impl MempoolController {
    // TODO : rename function
    pub async fn send_blocks_to_blockchain(&mut self) {
        let mut mempool = self.mempool.write().await;
        let mut blockchain = self.blockchain.write().await;
        while let Some(block) = mempool.blocks_queue.pop_front() {
            mempool.delete_transactions(&block.get_transactions());
            blockchain.add_block(block).await;
        }
    }
}

#[async_trait]
impl ProcessEvent<MempoolEvent> for MempoolController {
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()> {
        trace!("processing new global event");
        None
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()> {
        trace!("processing new interface event");
        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        trace!("processing timer event : {:?}", duration.as_micros());

        let mut can_bundle = false;
        let timestamp = self.io_handler.get_timestamp();
        // TODO : create mock transactions here
        {
            let mempool = self.mempool.read().await;
            can_bundle = mempool
                .can_bundle_block(self.blockchain.clone(), timestamp)
                .await;
        }
        if can_bundle {
            let mempool = self.mempool.clone();
            let mut mempool = mempool.write().await;
            let result = mempool
                .bundle_block(self.blockchain.clone(), timestamp)
                .await;
            mempool.add_block(result);
            self.send_blocks_to_blockchain();
        }
        None
    }

    async fn process_event(&mut self, event: MempoolEvent) -> Option<()> {
        None
    }
}

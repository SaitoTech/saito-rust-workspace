use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::consensus_event_processor::ConsensusEvent;
use crate::core::data::crypto::{generate_random_bytes, hash};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::wallet::Wallet;
use crate::core::routing_event_processor::RoutingEvent;
use crate::{log_read_lock_receive, log_read_lock_request};

const MINER_INTERVAL: u128 = 1_000;

#[derive(Debug)]
pub enum MiningEvent {
    LongestChainBlockAdded { hash: SaitoHash, difficulty: u64 },
}

/// Manages the miner
pub struct MiningEventProcessor {
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_blockchain: Sender<RoutingEvent>,
    pub sender_to_mempool: Sender<ConsensusEvent>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub miner_timer: u128,
    pub miner_active: bool,
    pub target: SaitoHash,
    pub difficulty: u64,
}

impl MiningEventProcessor {
    async fn mine(&mut self) {
        trace!("mining for golden ticket");

        assert!(self.miner_active);

        let public_key: SaitoPublicKey;

        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet.read().await;
            log_read_lock_receive!("wallet");
            public_key = wallet.public_key;
        }

        let random_bytes = hash(&generate_random_bytes(32));
        // The new way of validation will be wasting a GT instance if the validation fails
        // old way used a static method instead
        let gt = GoldenTicket::create(self.target, random_bytes, public_key);
        if gt.validate(self.difficulty) {
            info!(
                "golden ticket found. sending to mempool. previous block : {:?} random : {:?} key : {:?} solution : {:?} for difficulty : {:?}",
                hex::encode(gt.target),
                hex::encode(gt.random),
                hex::encode(gt.public_key),
                hex::encode(hash(&gt.serialize_for_net())),
                self.difficulty
            );
            self.miner_active = false;
            self.sender_to_mempool
                .send(ConsensusEvent::NewGoldenTicket { golden_ticket: gt })
                .await
                .expect("sending to mempool failed");
        }
    }
}

#[async_trait]
impl ProcessEvent<MiningEvent> for MiningEventProcessor {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        // trace!("processing new interface event");

        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        // trace!("processing timer event : {:?}", duration.as_micros());

        self.miner_timer += duration.as_micros();
        if self.miner_active {
            if self.miner_timer >= MINER_INTERVAL {
                self.miner_timer = 0;
                self.mine().await;
            }
        }

        None
    }

    async fn process_event(&mut self, event: MiningEvent) -> Option<()> {
        // debug!("event received : {:?}", event);
        match event {
            MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                debug!(
                    "Setting miner hash : {:?} and difficulty : {:?}",
                    hex::encode(hash),
                    difficulty
                );
                self.difficulty = difficulty;
                self.target = hash;
                self.miner_active = true;
            }
        }
        None
    }

    async fn on_init(&mut self) {}
}

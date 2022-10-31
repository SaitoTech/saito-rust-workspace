use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tracing::info;

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoHash, SaitoPublicKey, Timestamp};
use crate::common::keep_time::KeepTime;
use crate::common::process_event::ProcessEvent;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::data::crypto::{generate_random_bytes, hash};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::wallet::Wallet;
use crate::{log_read_lock_receive, log_read_lock_request};

#[derive(Debug)]
pub enum MiningEvent {
    LongestChainBlockAdded { hash: SaitoHash, difficulty: u64 },
}

/// Manages the miner
pub struct MiningThread {
    pub wallet: Arc<RwLock<Wallet>>,
    pub sender_to_mempool: Sender<ConsensusEvent>,
    pub time_keeper: Box<dyn KeepTime + Send + Sync>,
    pub miner_active: bool,
    pub target: SaitoHash,
    pub difficulty: u64,
    pub public_key: SaitoPublicKey,
    pub mined_golden_tickets: u64,
    pub stat_sender: Sender<String>,
}

impl MiningThread {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn mine(&mut self) {
        assert!(self.miner_active);
        debug_assert_ne!(self.public_key, [0; 33]);

        let random_bytes = hash(&generate_random_bytes(32));
        // The new way of validation will be wasting a GT instance if the validation fails
        // old way used a static method instead
        let gt = GoldenTicket::create(self.target, random_bytes, self.public_key);
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
            self.mined_golden_tickets += 1;
            self.sender_to_mempool
                .send(ConsensusEvent::NewGoldenTicket { golden_ticket: gt })
                .await
                .expect("sending to mempool failed");
        }
    }
}

#[async_trait]
impl ProcessEvent<MiningEvent> for MiningThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        unreachable!();
        None
    }

    async fn process_timer_event(&mut self, _duration: Duration) -> Option<()> {
        if self.miner_active {
            self.mine().await;
            return Some(());
        }

        None
    }

    async fn process_event(&mut self, event: MiningEvent) -> Option<()> {
        // debug!("event received : {:?}", event);
        return match event {
            MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                info!(
                    "Activating miner with hash : {:?} and difficulty : {:?}",
                    hex::encode(hash),
                    difficulty
                );
                self.difficulty = difficulty;
                self.target = hash;
                self.miner_active = true;
                Some(())
            }
        };
    }

    async fn on_init(&mut self) {
        log_read_lock_request!("wallet");
        let wallet = self.wallet.read().await;
        log_read_lock_receive!("wallet");
        self.public_key = wallet.public_key.clone();
    }

    async fn on_stat_interval(&mut self, _current_time: Timestamp) {
        let stat = format!("--- stats ------ {} - total : {:?} current difficulty : {:?} miner_active : {:?} current target : {:?} ",
                           format!("{:width$}", "mining::golden_tickets", width = 30),
                           self.mined_golden_tickets,
                           self.difficulty,
                           self.miner_active,
                           hex::encode(self.target));
        self.stat_sender.send(stat).await.unwrap();
    }
}

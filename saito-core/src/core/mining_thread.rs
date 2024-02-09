use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::info;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::core::consensus::golden_ticket::GoldenTicket;
use crate::core::consensus::wallet::Wallet;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::defs::{
    push_lock, PrintForLog, SaitoHash, SaitoPublicKey, Timestamp, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_WALLET,
};
use crate::core::io::network_event::NetworkEvent;
use crate::core::process::keep_time::KeepTime;
use crate::core::process::process_event::ProcessEvent;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::{generate_random_bytes, hash};
use crate::lock_for_read;

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
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    // todo : make this private and init using configs
    pub enabled: bool,
    pub mining_iterations: u32,
}

impl MiningThread {
    async fn mine(&mut self) -> bool {
        assert!(self.miner_active);

        if self.public_key == [0; 33] {
            let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
            if wallet.public_key == [0; 33] {
                // wallet not initialized yet
                return false;
            }
            self.public_key = wallet.public_key;
            info!("node public key = {:?}", self.public_key.to_base58());
        }

        let random_bytes = hash(&generate_random_bytes(32));
        // The new way of validation will be wasting a GT instance if the validation fails
        // old way used a static method instead
        let gt = GoldenTicket::create(self.target, random_bytes, self.public_key);
        if gt.validate(self.difficulty) {
            info!(
                "golden ticket found. sending to mempool. previous block : {:?} random : {:?} key : {:?} solution : {:?} for difficulty : {:?}",
                gt.target.to_hex(),
                gt.random.to_hex(),
                gt.public_key.to_base58(),
                hash(&gt.serialize_for_net()).to_hex(),
                self.difficulty
            );
            self.miner_active = false;
            self.mined_golden_tickets += 1;
            self.sender_to_mempool
                .send(ConsensusEvent::NewGoldenTicket { golden_ticket: gt })
                .await
                .expect("sending to mempool failed");
            return true;
        }
        false
    }
}

#[async_trait]
impl ProcessEvent<MiningEvent> for MiningThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        unreachable!();
    }

    async fn process_timer_event(&mut self, _duration: Duration) -> Option<()> {
        if !self.enabled {
            return None;
        }
        if self.enabled && self.miner_active {
            for _ in 0..self.mining_iterations {
                if self.mine().await {
                    return Some(());
                }
            }
            return Some(());
        }

        None
    }

    async fn process_event(&mut self, event: MiningEvent) -> Option<()> {
        if !self.enabled {
            return None;
        }
        return match event {
            MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                info!(
                    "Activating miner with hash : {:?} and difficulty : {:?}",
                    hash.to_hex(),
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
        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        info!("is browser = {:?}", configs.is_browser());
        self.enabled = !configs.is_browser();
        info!("miner is enabled");
        let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
        self.public_key = wallet.public_key;
        info!("node public key = {:?}", self.public_key.to_base58());
    }

    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        if !self.enabled {
            return;
        }
        let stat = format!("{} - {} - total : {:?}, current difficulty : {:?}, miner_active : {:?}, current target : {:?} ",
                           current_time,
                           format!("{:width$}", "mining::golden_tickets", width = 40),
                           self.mined_golden_tickets,
                           self.difficulty,
                           self.miner_active,
                           self.target.to_hex());
        self.stat_sender.send(stat).await.unwrap();
    }
}

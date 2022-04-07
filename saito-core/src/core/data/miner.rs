use std::io::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::process_event::ProcessEvent;
use crate::common::run_task::RunTask;
use crate::core::data::crypto::{generate_random_bytes, hash};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::wallet::Wallet;
use crate::core::mempool_controller::MempoolEvent;

pub struct Miner {
    time_to_next_tick: u128,
    // pub target: SaitoHash,
    // pub difficulty: u64,
    pub wallet: Arc<RwLock<Wallet>>,
}

impl Miner {
    pub fn new(wallet: Arc<RwLock<Wallet>>) -> Miner {
        Miner {
            time_to_next_tick: 0,
            // target: [0; 32],
            // difficulty: 0,
            wallet,
        }
    }
    pub fn init(&mut self, task_runner: &dyn RunTask) -> Result<(), Error> {
        debug!("Miner.init");
        Ok(())
    }
    pub fn on_timer(&mut self, duration: Duration) -> Option<()> {
        trace!("Miner.on_timer");
        None
    }
    pub async fn mine(
        &self,
        target: SaitoHash,
        difficulty: u64,
        sender_to_mempool: Sender<MempoolEvent>,
    ) {
        trace!("Miner.mine");
        // TODO : remove this if not required
        // self.time_to_next_tick = self.time_to_next_tick + duration.as_micros();
        // if self.time_to_next_tick >= 5_000_000 {
        //     self.time_to_next_tick = 0;
        // }
        //
        // if self.time_to_next_tick > 0 {
        //     return;
        // }
        //
        // info!("block created");

        let publickey: SaitoPublicKey;

        {
            let wallet = self.wallet.read().await;
            publickey = wallet.get_publickey();
        }

        let random_bytes = hash(&generate_random_bytes(32));
        let solution = GoldenTicket::generate_solution(target, random_bytes, publickey);
        if GoldenTicket::is_valid_solution(solution, difficulty) {
            let gt = GoldenTicket::new(target, random_bytes, publickey);

            let result = sender_to_mempool
                .send(MempoolEvent::NewGoldenTicket { golden_ticket: gt })
                .await;
            // TODO : check result
        }
    }
}

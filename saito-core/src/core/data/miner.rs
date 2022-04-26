use std::io::Error;
use std::sync::Arc;
use std::time::Duration;

use log::{debug, info, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey};
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
        debug!("mining for golden ticket");
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
            trace!("waiting for the wallet read lock");
            let wallet = self.wallet.read().await;
            trace!("acquired the wallet read lock");
            publickey = wallet.get_publickey();
        }
        let random_bytes = hash(&generate_random_bytes(32));
        let solution = GoldenTicket::generate_solution(target, random_bytes, publickey);
        if GoldenTicket::is_valid_solution(solution, difficulty) {
            let gt = GoldenTicket::new(target, random_bytes, publickey);

            debug!(
                "golden ticket found. sending to mempool : {:?}",
                hex::encode(gt.get_target())
            );
            let result = sender_to_mempool
                .send(MempoolEvent::NewGoldenTicket { golden_ticket: gt })
                .await;
            trace!("sent to mempool");
            // TODO : check result
        }
    }
    // TODO : move this to test manager
    // function used primarily for test functions
    pub async fn mine_on_block_until_golden_ticket_found(
        &mut self,
        block_hash: SaitoHash,
        block_difficulty: u64,
    ) -> GoldenTicket {
        let publickey;
        {
            trace!("waiting for the wallet read lock");
            let wallet = self.wallet.read().await;
            trace!("acquired the wallet read lock");
            publickey = wallet.get_publickey();
        }
        let mut random_bytes = hash(&generate_random_bytes(32));

        let mut solution = GoldenTicket::generate_solution(block_hash, random_bytes, publickey);

        while !GoldenTicket::is_valid_solution(solution, block_difficulty) {
            random_bytes = hash(&generate_random_bytes(32));
            solution = GoldenTicket::generate_solution(block_hash, random_bytes, publickey);
        }

        GoldenTicket::new(block_hash, random_bytes, publickey)
    }
}

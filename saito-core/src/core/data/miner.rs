use std::sync::Arc;

use log::{debug, trace};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey};

use crate::core::blockchain_controller::MempoolEvent;
use crate::core::data::crypto::{generate_random_bytes, hash};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::wallet::Wallet;

pub struct Miner {
    pub target: SaitoHash,
    pub difficulty: u64,
    pub wallet: Arc<RwLock<Wallet>>,
}

impl Miner {
    pub fn new(wallet: Arc<RwLock<Wallet>>) -> Miner {
        Miner {
            target: [0; 32],
            difficulty: 0,
            wallet,
        }
    }

    pub async fn mine(&self, sender_to_mempool: Sender<MempoolEvent>) {
        debug!("mining for golden ticket");

        let publickey: SaitoPublicKey;

        {
            trace!("waiting for the wallet read lock");
            let wallet = self.wallet.read().await;
            trace!("acquired the wallet read lock");
            publickey = wallet.get_publickey();
        }
        let random_bytes = hash(&generate_random_bytes(32));
        let solution = GoldenTicket::generate_solution(self.target, random_bytes, publickey);
        if GoldenTicket::is_valid_solution(solution, self.difficulty) {
            let gt = GoldenTicket::new(self.target, random_bytes, publickey);

            debug!(
                "golden ticket found. sending to mempool : {:?}",
                hex::encode(gt.get_target())
            );
            sender_to_mempool
                .send(MempoolEvent::NewGoldenTicket { golden_ticket: gt })
                .await
                .expect("sending to mempool failed");
        }
    }
}

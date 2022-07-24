use std::sync::Arc;
use std::time::Duration;

use crate::saito::mempool_handler::ConsensusEvent;
use log::{debug, info, trace};
use saito_core::common::defs::{SaitoHash, SaitoPublicKey};
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data::crypto::{generate_random_bytes, hash};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_event_processor::MiningEvent;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;

#[derive(Debug)]
// pub enum MiningEvent {
//     LongestChainBlockAdded { hash: SaitoHash, difficulty: u64 },
// }
pub struct MiningHandler {
    pub wallet: Arc<RwLock<Wallet>>,
    pub event_receiver: Receiver<MiningEvent>,
    pub sender_to_mempool: Sender<ConsensusEvent>,
}

impl MiningHandler {
    pub async fn run(mut handler: MiningHandler) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("Mining Handler is Running");
            handler.run_event_loop();
        })
    }

    async fn run_event_loop(&mut self) {
        loop {
            if let Some(event) = self.event_receiver.recv().await {
                debug!("event received : {:?}", event);
                match event {
                    MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                        self.on_block_added_to_the_longest_chain(hash, difficulty)
                            .await;
                    }
                }
            }
        }
    }

    async fn on_block_added_to_the_longest_chain(&mut self, hash: SaitoHash, difficulty: u64) {
        let wallet = self.wallet.clone();
        let sender = self.sender_to_mempool.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;

                if let Some(golden_ticket) =
                    MiningHandler::find_solution(hash, difficulty, &wallet, &sender).await
                {
                    sender
                        .send(ConsensusEvent::NewGoldenTicket { golden_ticket })
                        .await
                        .expect("sending to mempool failed");
                    break;
                }
            }
        });
    }

    async fn find_solution(
        target: SaitoHash,
        difficulty: u64,
        wallet: &Arc<RwLock<Wallet>>,
        sender: &Sender<ConsensusEvent>,
    ) -> Option<GoldenTicket> {
        debug!("mining for golden ticket");

        let publickey: SaitoPublicKey;
        {
            trace!("waiting for the wallet read lock");
            let wallet = wallet.read().await;
            trace!("acquired the wallet read lock");
            publickey = wallet.public_key;
        }

        let random_bytes = hash(&generate_random_bytes(32));
        let gt = GoldenTicket::create(target, random_bytes, publickey);

        if gt.validate(difficulty) {
            debug!(
                "golden ticket found. sending to mempool : {:?}",
                hex::encode(gt.target)
            );

            return Some(gt);
        }

        return None;
    }
}

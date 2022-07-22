use std::sync::Arc;
use std::time::Duration;

use crate::saito::consensus_handler::ConsensusEvent;
use log::{debug, trace};
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
    pub sender_to_consensus_handler: Sender<ConsensusEvent>,
}

impl MiningHandler {
    pub async fn run(
        mut handler: MiningHandler,
        mut event_receiver: Receiver<MiningEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let result = event_receiver.recv().await;
                if result.is_some() {
                    let event = result.unwrap();
                    handler.process_event(event).await;
                }
            }
        })
    }

    async fn process_event(&mut self, event: MiningEvent) {
        debug!("event received : {:?}", event);
        match event {
            MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                let wallet = self.wallet.clone();
                let sender = self.sender_to_consensus_handler.clone();

                tokio::spawn(async move {
                    let mut interval = interval(Duration::from_millis(100));
                    loop {
                        interval.tick().await;
                        if MiningHandler::mine(hash, difficulty, &wallet, &sender).await {
                            break;
                        }
                    }
                });
            }
        }
    }

    async fn mine(
        target: SaitoHash,
        difficulty: u64,
        wallet: &Arc<RwLock<Wallet>>,
        sender: &Sender<ConsensusEvent>,
    ) -> bool {
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

            sender
                .send(ConsensusEvent::NewGoldenTicket { golden_ticket: gt })
                .await
                .expect("sending to mempool failed");

            return true;
        }

        return false;
    }
}

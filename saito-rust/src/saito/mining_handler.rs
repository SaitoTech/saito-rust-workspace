use std::sync::Arc;
use std::time::Duration;

use crate::saito::mempool_handler::MempoolEvent;
use log::{debug, info, trace};
use saito_core::common::defs::{SaitoHash, SaitoPublicKey};
use saito_core::core::data::context::Context;
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
    wallet: Arc<RwLock<Wallet>>,
    event_receiver: Receiver<MiningEvent>,
    sender_to_mempool: Sender<MempoolEvent>,
}

impl MiningHandler {
    pub fn new(
        context: &Context,
        event_receiver: Receiver<MiningEvent>,
        sender_to_mempool: Sender<MempoolEvent>,
    ) -> Self {
        MiningHandler {
            wallet: context.wallet.clone(),
            event_receiver,
            sender_to_mempool,
        }
    }

    pub async fn run(mut handler: MiningHandler) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("Mining Handler is Running");
            handler.run_event_loop().await;
        })
    }

    async fn run_event_loop(&mut self) {
        loop {
            if let Some(event) = self.event_receiver.recv().await {
                debug!("event received : {:?}", event);
                match event {
                    MiningEvent::LongestChainBlockAdded { hash, difficulty } => {
                        self.on_longest_chain_block_added(hash, difficulty).await;
                    }
                }
            }
        }
    }

    async fn on_longest_chain_block_added(&mut self, hash: SaitoHash, difficulty: u64) {
        let wallet = self.wallet.clone();
        let sender = self.sender_to_mempool.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;

                if let Some(golden_ticket) =
                    MiningHandler::find_solution(hash, difficulty, &wallet).await
                {
                    sender
                        .send(MempoolEvent::NewGoldenTicket { golden_ticket })
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

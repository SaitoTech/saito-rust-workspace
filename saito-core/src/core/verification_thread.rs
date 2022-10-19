use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rayon::prelude::*;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tracing::debug;

use crate::common::command::NetworkEvent;
use crate::common::defs::{SaitoPublicKey, StatVariable, Timestamp};
use crate::common::process_event::ProcessEvent;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::{log_read_lock_receive, log_read_lock_request};

#[derive(Debug)]
pub enum VerifyRequest {
    Transaction(Transaction),
    Transactions(VecDeque<Transaction>),
    Block(Vec<u8>, u64),
}

pub struct VerificationThread {
    pub sender_to_consensus: Sender<ConsensusEvent>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub wallet: Arc<RwLock<Wallet>>,
    pub public_key: SaitoPublicKey,
    pub processed_txs: StatVariable,
    pub processed_blocks: StatVariable,
    pub processed_msgs: StatVariable,
    pub invalid_txs: StatVariable,
}

impl VerificationThread {
    pub async fn verify_tx(&mut self, mut transaction: Transaction) {
        {
            transaction.generate(&self.public_key, 0, 0);

            log_read_lock_request!("VerificationThread:verify_tx::blockchain");
            let blockchain = self.blockchain.read().await;
            log_read_lock_receive!("VerificationThread:verify_tx::blockchain");
            if !transaction.validate(&blockchain.utxoset) {
                debug!(
                    "transaction : {:?} not valid",
                    hex::encode(transaction.signature)
                );
                self.processed_txs.increment();
                return;
            }
        }

        self.processed_txs.increment();
        self.processed_msgs.increment();
        self.sender_to_consensus
            .send(ConsensusEvent::NewTransaction { transaction })
            .await
            .unwrap();
    }
    pub async fn verify_txs(&mut self, transactions: &mut VecDeque<Transaction>) {
        self.processed_txs.increment_by(transactions.len() as u64);
        self.processed_msgs.increment_by(transactions.len() as u64);
        let prev_count = transactions.len();
        let txs: Vec<Transaction>;
        {
            log_read_lock_request!("VerificationThread:verify_txs::blockchain");
            let blockchain = self.blockchain.read().await;
            log_read_lock_receive!("VerificationThread:verify_txs::blockchain");
            txs = transactions
                .par_drain(..)
                .with_min_len(10)
                // .with_max_len(1000)
                .filter_map(|mut transaction| {
                    transaction.generate(&self.public_key, 0, 0);

                    if !transaction.validate(&blockchain.utxoset) {
                        debug!(
                            "transaction : {:?} not valid",
                            hex::encode(transaction.signature)
                        );

                        return None;
                    }
                    return Some(transaction);
                })
                .collect();
        }

        let invalid_txs = prev_count - txs.len();
        for transaction in txs {
            self.sender_to_consensus
                .send(ConsensusEvent::NewTransaction { transaction })
                .await
                .unwrap();
        }
        self.invalid_txs.increment_by(invalid_txs as u64);
    }
    pub async fn verify_block(&mut self, buffer: Vec<u8>, peer_index: u64) {
        let mut block = Block::deserialize_from_net(&buffer);
        log_read_lock_request!("RoutingEventProcessor:process_network_event::peers");
        let peers = self.peers.read().await;
        log_read_lock_receive!("RoutingEventProcessor:process_network_event::peers");
        let peer = peers.index_to_peers.get(&peer_index);
        if peer.is_some() {
            let peer = peer.unwrap();
            block.source_connection_id = peer.public_key.clone();
        }
        block.generate();
        self.processed_blocks.increment();
        self.processed_msgs.increment();

        self.sender_to_consensus
            .send(ConsensusEvent::BlockFetched { peer_index, block })
            .await
            .unwrap();
    }
    // async fn on_init(&mut self) {
    //     log_read_lock_request!("VerificationThread:on_init::wallet");
    //     let wallet = self.wallet.read().await;
    //     log_read_lock_receive!("VerificationThread:on_init::wallet");
    //     self.public_key = wallet.public_key.clone();
    // }
    // async fn run(&mut self) {
    //     let mut work_done = false;
    //     let batch_count = 1000;
    //     loop {
    //         work_done = false;
    //
    //
    //         if !work_done {
    //             tokio::time::sleep(Duration::from_millis(10)).await;
    //         }
    //     }
    // }
}

#[async_trait]
impl ProcessEvent<VerifyRequest> for VerificationThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        None
    }

    async fn process_timer_event(&mut self, _duration: Duration) -> Option<()> {
        None
    }

    async fn process_event(&mut self, request: VerifyRequest) -> Option<()> {
        match request {
            VerifyRequest::Transaction(transaction) => {
                self.verify_tx(transaction).await;
            }
            VerifyRequest::Block(block, peer_index) => {
                self.verify_block(block, peer_index).await;
            }
            VerifyRequest::Transactions(mut txs) => {
                self.verify_txs(&mut txs).await;
            }
        }

        Some(())
    }

    async fn on_init(&mut self) {
        log_read_lock_request!("VerificationThread:on_init::wallet");
        let wallet = self.wallet.read().await;
        log_read_lock_receive!("VerificationThread:on_init::wallet");
        self.public_key = wallet.public_key.clone();
    }

    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        self.processed_msgs.calculate_stats(current_time);
        self.invalid_txs.calculate_stats(current_time);
        // self.processed_txs.calculate_stats(current_time);
        // self.processed_blocks.calculate_stats(current_time);

        self.processed_msgs.print();
        self.invalid_txs.print();
        // self.processed_txs.print();
        // self.processed_blocks.print();
    }
}

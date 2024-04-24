use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, warn};
use rayon::prelude::*;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::core::consensus::block::Block;
use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::peer_collection::PeerCollection;
use crate::core::consensus::transaction::Transaction;
use crate::core::consensus::wallet::Wallet;
use crate::core::consensus_thread::ConsensusEvent;
use crate::core::defs::{PrintForLog, StatVariable, Timestamp};
use crate::core::io::network_event::NetworkEvent;
use crate::core::process::process_event::ProcessEvent;
use crate::drain;

#[derive(Debug)]
pub enum VerifyRequest {
    Transaction(Transaction),
    Transactions(VecDeque<Transaction>),
    Block(Vec<u8>, u64),
}

pub struct VerificationThread {
    pub sender_to_consensus: Sender<ConsensusEvent>,
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub peer_lock: Arc<RwLock<PeerCollection>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub processed_txs: StatVariable,
    pub processed_blocks: StatVariable,
    pub processed_msgs: StatVariable,
    pub invalid_txs: StatVariable,
    pub stat_sender: Sender<String>,
}

impl VerificationThread {
    pub async fn verify_tx(&mut self, mut transaction: Transaction) {
        let public_key;
        {
            let wallet = self.wallet_lock.read().await;
            public_key = wallet.public_key;
        }
        {
            transaction.generate(&public_key, 0, 0);

            let blockchain = self.blockchain_lock.read().await;

            if !transaction.validate(&blockchain.utxoset) {
                debug!(
                    "transaction : {:?} not valid",
                    transaction.signature.to_hex()
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
            let blockchain = self.blockchain_lock.read().await;

            let public_key;
            {
                let wallet = self.wallet_lock.read().await;
                public_key = wallet.public_key;
            }
            txs = drain!(transactions, 10)
                .filter_map(|mut transaction| {
                    transaction.generate(&public_key, 0, 0);

                    if !transaction.validate(&blockchain.utxoset) {
                        debug!(
                            "transaction : {:?} not valid",
                            transaction.signature.to_hex()
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
        // trace!("verifying block buffer of size : {:?}", buffer.len());
        let buffer_len = buffer.len();
        let result = Block::deserialize_from_net(buffer);
        if result.is_err() {
            warn!(
                "failed verifying block buffer with length : {:?}",
                buffer_len
            );
            return;
        }
        let peers = self.peer_lock.read().await;

        let mut block = result.unwrap();
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
}

#[async_trait]
impl ProcessEvent<VerifyRequest> for VerificationThread {
    async fn process_network_event(&mut self, _event: NetworkEvent) -> Option<()> {
        unreachable!();
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

    async fn on_init(&mut self) {}

    async fn on_stat_interval(&mut self, current_time: Timestamp) {
        self.processed_msgs.calculate_stats(current_time).await;
        self.invalid_txs.calculate_stats(current_time).await;
        self.processed_txs.calculate_stats(current_time).await;
        self.processed_blocks.calculate_stats(current_time).await;
    }
}

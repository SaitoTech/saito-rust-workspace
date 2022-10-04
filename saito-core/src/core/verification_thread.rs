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
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tracing::debug;

#[derive(Debug)]
pub enum VerifyRequest {
    Transaction(Transaction),
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
}

impl VerificationThread {
    async fn verify_tx(&mut self, mut transaction: Transaction) {
        self.processed_txs.increment();
        {
            transaction.generate(&self.public_key, 0, 0);

            log_read_lock_request!("RoutingEventProcessor:process_incoming_message::blockchain");
            let blockchain = self.blockchain.read().await;
            log_read_lock_receive!("RoutingEventProcessor:process_incoming_message::blockchain");
            if !transaction.validate(&blockchain.utxoset) {
                debug!(
                    "transaction : {:?} not valid",
                    hex::encode(transaction.signature)
                );
                return;
            }
        }

        self.sender_to_consensus
            .send(ConsensusEvent::NewTransaction { transaction })
            .await
            .unwrap();
    }
    async fn verify_block(&mut self, buffer: Vec<u8>, peer_index: u64) {
        self.processed_blocks.increment();
        let mut block = Block::deserialize_from_net(&buffer);

        log_read_lock_request!("RoutingEventProcessor:process_network_event::peers");
        let peers = self.peers.read().await;
        log_read_lock_receive!("RoutingEventProcessor:process_network_event::peers");
        let peer = peers.index_to_peers.get(&peer_index);
        if peer.is_some() {
            let peer = peer.unwrap();
            block.source_connection_id = Some(peer.public_key);
        }
        block.generate();

        self.sender_to_consensus
            .send(ConsensusEvent::BlockFetched { peer_index, block })
            .await
            .unwrap();
    }
}

#[async_trait]
impl ProcessEvent<VerifyRequest> for VerificationThread {
    async fn process_network_event(&mut self, event: NetworkEvent) -> Option<()> {
        None
    }

    async fn process_timer_event(&mut self, duration: Duration) -> Option<()> {
        None
    }

    async fn process_event(&mut self, request: VerifyRequest) -> Option<()> {
        self.processed_msgs.increment();
        match request {
            VerifyRequest::Transaction(transaction) => {
                self.verify_tx(transaction).await;
            }
            VerifyRequest::Block(block, peer_index) => {
                self.verify_block(block, peer_index).await;
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
        // self.processed_txs.calculate_stats(current_time);
        // self.processed_blocks.calculate_stats(current_time);

        self.processed_msgs.print();
        // self.processed_txs.print();
        // self.processed_blocks.print();
    }
}

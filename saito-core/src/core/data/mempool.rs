use std::{collections::HashMap, collections::VecDeque, sync::Arc};

use log::{debug, info, trace};
use tokio::sync::{broadcast, RwLock};

use crate::common::command::GlobalEvent;
use crate::common::defs::{SaitoHash, SaitoPrivateKey, SaitoPublicKey};
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::burnfee::BurnFee;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

//
// In addition to responding to global broadcast messages, the
// mempool has a local broadcast channel it uses to coordinate
// attempts to bundle blocks and notify itself when a block has
// been produced.
//
#[derive(Clone, Debug)]
pub enum MempoolMessage {
    LocalTryBundleBlock,
    LocalNewBlock,
}

/// The `Mempool` holds unprocessed blocks and transactions and is in control of
/// discerning when the node is allowed to create a block. It bundles the block and
/// sends it to the `Blockchain` to be added to the longest-chain. New `Block`s
/// received over the network are queued in the `Mempool` before being added to
/// the `Blockchain`
#[derive(Debug)]
pub struct Mempool {
    pub(crate) blocks_queue: VecDeque<Block>,
    pub transactions: Vec<Transaction>,
    // vector so we just copy it over
    routing_work_in_mempool: u64,
    wallet_lock: Arc<RwLock<Wallet>>,
    broadcast_channel_sender: Option<broadcast::Sender<GlobalEvent>>,
    mempool_publickey: SaitoPublicKey,
    mempool_privatekey: SaitoPrivateKey,
}

impl Mempool {
    #[allow(clippy::new_without_default)]
    pub fn new(wallet_lock: Arc<RwLock<Wallet>>) -> Self {
        Mempool {
            blocks_queue: VecDeque::new(),
            transactions: vec![],
            routing_work_in_mempool: 0,
            wallet_lock,
            broadcast_channel_sender: None,
            mempool_publickey: [0; 33],
            mempool_privatekey: [0; 32],
        }
    }

    pub fn add_block(&mut self, block: Block) {
        debug!("mempool add block : {:?}", hex::encode(block.get_hash()));
        let hash_to_insert = block.get_hash();
        if self
            .blocks_queue
            .iter()
            .any(|block| block.get_hash() == hash_to_insert)
        {
            // do nothing
        } else {
            self.blocks_queue.push_back(block);
        }
    }
    pub async fn add_golden_ticket(&mut self, golden_ticket: GoldenTicket) {
        debug!("adding golden ticket");
        let mut wallet = self.wallet_lock.write().await;
        let transaction = wallet.create_golden_ticket_transaction(golden_ticket).await;

        if self
            .transactions
            .iter()
            .any(|transaction| transaction.is_golden_ticket())
        {
        } else {
            info!("adding golden ticket to mempool...");
            self.transactions.push(transaction);
        }
    }

    pub async fn add_transaction_if_validates(
        &mut self,
        transaction: Transaction,
        blockchain: &Blockchain,
    ) {
        //
        // validate
        //
        if transaction.validate(&blockchain.utxoset, &blockchain.staking) {
            self.add_transaction(transaction).await;
        }
    }
    pub async fn add_transaction(&mut self, mut transaction: Transaction) {
        trace!("add_transaction {:?}", transaction.get_transaction_type());
        let tx_sig_to_insert = transaction.get_signature();

        //
        // this assigns the amount of routing work that this transaction
        // contains to us, which is why we need to provide our publickey
        // so that we can calculate routing work.
        //
        let publickey;
        {
            let wallet = self.wallet_lock.read().await;
            publickey = wallet.get_publickey();
        }
        transaction.generate_metadata(publickey);

        let routing_work_available_for_me =
            transaction.get_routing_work_for_publickey(self.mempool_publickey);

        if self
            .transactions
            .iter()
            .any(|transaction| transaction.get_signature() == tx_sig_to_insert)
        {
        } else {
            self.transactions.push(transaction);
            self.routing_work_in_mempool += routing_work_available_for_me;
        }
    }

    pub async fn bundle_block(
        &mut self,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        current_timestamp: u64,
    ) -> Block {
        debug!("bundling block...");
        let previous_block_hash: SaitoHash;
        {
            let blockchain = blockchain_lock.read().await;
            previous_block_hash = blockchain.get_latest_block_hash();
        }

        let mut block = Block::generate(
            &mut self.transactions,
            previous_block_hash,
            self.wallet_lock.clone(),
            blockchain_lock.clone(),
            current_timestamp,
        )
        .await;
        block.generate_metadata();

        self.routing_work_in_mempool = 0;

        block
    }

    pub async fn can_bundle_block(
        &self,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        current_timestamp: u64,
    ) -> bool {
        if self.transactions.is_empty() {
            return false;
        }
        debug!("can bundle block");

        let blockchain = blockchain_lock.read().await;

        if let Some(previous_block) = blockchain.get_latest_block() {
            let work_available = self.get_routing_work_available();
            let work_needed = self.get_routing_work_needed(previous_block, current_timestamp);
            let time_elapsed = current_timestamp - previous_block.get_timestamp();
            info!(
                "can_bundle_block. work available: {:?} -- work needed: {:?} -- time elapsed: {:?} ",
                work_available,
                work_needed,
                time_elapsed
            );
            work_available >= work_needed
        } else {
            true
        }
    }

    pub fn delete_transactions(&mut self, transactions: &Vec<Transaction>) {
        let mut tx_hashmap = HashMap::new();
        for transaction in transactions {
            let hash = transaction.get_hash_for_signature();
            tx_hashmap.entry(hash).or_insert(true);
        }

        self.routing_work_in_mempool = 0;

        self.transactions
            .retain(|x| tx_hashmap.contains_key(&x.get_hash_for_signature()) != true);

        for transaction in &self.transactions {
            self.routing_work_in_mempool +=
                transaction.get_routing_work_for_publickey(self.mempool_publickey);
        }
    }

    ///
    /// Calculates the work available in mempool to produce a block
    ///
    pub fn get_routing_work_available(&self) -> u64 {
        if self.routing_work_in_mempool > 0 {
            return self.routing_work_in_mempool;
        }
        0
    }

    //
    // Return work needed in Nolan
    //
    pub fn get_routing_work_needed(&self, previous_block: &Block, current_timestamp: u64) -> u64 {
        let previous_block_timestamp = previous_block.get_timestamp();
        let previous_block_burnfee = previous_block.get_burnfee();

        let work_needed: u64 = BurnFee::return_routing_work_needed_to_produce_block_in_nolan(
            previous_block_burnfee,
            current_timestamp,
            previous_block_timestamp,
        );

        work_needed
    }

    pub fn set_broadcast_channel_sender(&mut self, bcs: broadcast::Sender<GlobalEvent>) {
        self.broadcast_channel_sender = Some(bcs);
    }

    pub fn set_mempool_publickey(&mut self, publickey: SaitoPublicKey) {
        self.mempool_publickey = publickey;
    }

    pub fn set_mempool_privatekey(&mut self, privatekey: SaitoPrivateKey) {
        self.mempool_privatekey = privatekey;
    }

    pub fn transaction_exists(&self, tx_hash: Option<SaitoHash>) -> bool {
        self.transactions
            .iter()
            .any(|transaction| transaction.get_hash_for_signature() == tx_hash)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::*;

    #[test]
    fn mempool_new_test() {
        let wallet = Wallet::new();
        let mempool = Mempool::new(Arc::new(RwLock::new(wallet)));
        assert_eq!(mempool.blocks_queue, VecDeque::new());
    }

    #[test]
    fn mempool_add_block_test() {
        let wallet = Wallet::new();
        let mut mempool = Mempool::new(Arc::new(RwLock::new(wallet)));
        let block = Block::new();
        mempool.add_block(block.clone());
        assert_eq!(Some(block), mempool.blocks_queue.pop_front())
    }
}

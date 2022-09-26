use std::time::Duration;
use std::{collections::HashMap, collections::VecDeque, sync::Arc};

use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};

use crate::common::defs::{SaitoHash, SaitoPrivateKey, SaitoPublicKey};
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::burnfee::BurnFee;
use crate::core::data::crypto::hash;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::transaction::{Transaction, TransactionType};
use crate::core::data::wallet::Wallet;
use crate::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

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
    pub blocks_queue: VecDeque<Block>,
    pub transactions: Vec<Transaction>,
    // vector so we just copy it over
    routing_work_in_mempool: u64,
    wallet_lock: Arc<RwLock<Wallet>>,
    mempool_public_key: SaitoPublicKey,
    mempool_private_key: SaitoPrivateKey,
    new_golden_ticket_added: bool,
    new_tx_added: bool,
}

impl Mempool {
    #[allow(clippy::new_without_default)]
    pub fn new(wallet_lock: Arc<RwLock<Wallet>>) -> Self {
        Mempool {
            blocks_queue: VecDeque::new(),
            transactions: vec![],
            routing_work_in_mempool: 0,
            wallet_lock,
            mempool_public_key: [0; 33],
            mempool_private_key: [0; 32],
            new_golden_ticket_added: false,
            new_tx_added: false,
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn add_block(&mut self, block: Block) {
        debug!("mempool add block : {:?}", hex::encode(block.hash));
        let hash_to_insert = block.hash;
        if !self
            .blocks_queue
            .iter()
            .any(|block| block.hash == hash_to_insert)
        {
            self.blocks_queue.push_back(block);
        } else {
            debug!("block not added to mempool as it was already there");
        }
    }
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn add_golden_ticket(&mut self, golden_ticket: GoldenTicket) {
        debug!(
            "adding golden ticket : {:?}",
            hex::encode(hash(&golden_ticket.serialize_for_net()))
        );
        let transaction;
        let target = golden_ticket.target;
        {
            log_write_lock_request!("wallet");
            let mut wallet = self.wallet_lock.write().await;
            log_write_lock_receive!("wallet");
            transaction = wallet.create_golden_ticket_transaction(golden_ticket).await;
        }
        for tx in self.transactions.iter() {
            if let TransactionType::GoldenTicket = tx.transaction_type {
                let gt = GoldenTicket::deserialize_from_net(&tx.message);
                if gt.target == target {
                    debug!("similar golden ticket already exists : {:?}", target);
                    return;
                }
            }
        }
        self.new_golden_ticket_added = true;
        // if !self
        //     .transactions
        //     .iter()
        //     .any(|transaction| transaction.is_golden_ticket())
        // {
        self.transactions.push(transaction);
        info!("golden ticket added to mempool");
        // }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn add_transaction_if_validates(
        &mut self,
        mut transaction: Transaction,
        blockchain: &Blockchain,
    ) {
        trace!(
            "add transaction if validates : {:?}",
            hex::encode(transaction.hash_for_signature.unwrap())
        );
        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet_lock.read().await;
            log_read_lock_receive!("wallet");
            transaction.generate(wallet.public_key);
        }
        //
        // validate
        //
        if transaction.validate(&blockchain.utxoset) {
            self.add_transaction(transaction).await;
        } else {
            debug!(
                "transaction not valid : {:?}",
                transaction.hash_for_signature.unwrap()
            );
        }
    }
    #[tracing::instrument(level = "info", skip_all)]
    async fn add_transaction(&mut self, mut transaction: Transaction) {
        trace!(
            "add_transaction {:?} : type = {:?}",
            hex::encode(transaction.hash_for_signature.unwrap()),
            transaction.transaction_type
        );
        let tx_sig_to_insert = transaction.signature;

        //
        // this assigns the amount of routing work that this transaction
        // contains to us, which is why we need to provide our public_key
        // so that we can calculate routing work.
        //
        let public_key;
        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet_lock.read().await;
            log_read_lock_receive!("wallet");
            public_key = wallet.public_key;
        }

        //
        // generates hashes, total fees, routing work for me, etc.
        //
        transaction.generate(public_key);

        if !self
            .transactions
            .iter()
            .any(|transaction| transaction.signature == tx_sig_to_insert)
        {
            self.routing_work_in_mempool += transaction.total_work;
            self.transactions.push(transaction);
            self.new_tx_added = true;
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn bundle_block(
        &mut self,
        blockchain: &mut Blockchain,
        current_timestamp: u64,
    ) -> Block {
        debug!("bundling block...");
        let previous_block_hash: SaitoHash;
        {
            previous_block_hash = blockchain.get_latest_block_hash();
        }

        let mut block = Block::create(
            &mut self.transactions,
            previous_block_hash,
            self.wallet_lock.clone(),
            blockchain,
            current_timestamp,
        )
        .await;
        block.generate();

        self.routing_work_in_mempool = 0;

        block
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn bundle_genesis_block(
        &mut self,
        blockchain: &mut Blockchain,
        current_timestamp: u64,
    ) -> Block {
        debug!("bundling genesis block...");

        let mut block = Block::create(
            &mut self.transactions,
            [0; 32],
            self.wallet_lock.clone(),
            blockchain,
            current_timestamp,
        )
        .await;
        block.generate();

        self.routing_work_in_mempool = 0;

        block
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn can_bundle_block(&self, blockchain: &Blockchain, current_timestamp: u64) -> bool {
        // if self.transactions.is_empty() {
        //     return false;
        // }
        trace!("can bundle block : timestamp = {:?}", current_timestamp);

        // TODO : add checks [downloading_active,etc...] from SLR code here

        if blockchain.blocks.is_empty() {
            warn!("Not generating #1 block. Waiting for blocks from peers");
            tokio::time::sleep(Duration::from_secs(1)).await;
            return false;
        }
        if !self.blocks_queue.is_empty() {
            trace!("waiting till blocks in the queue are processed");
            return false;
        }
        if self.transactions.is_empty() || !self.new_tx_added {
            trace!("waiting till transactions come in");
            return false;
        }
        if !blockchain.gt_requirement_met && !self.new_golden_ticket_added {
            trace!("waiting till more golden tickets come in");
            return false;
        }

        if let Some(previous_block) = blockchain.get_latest_block() {
            let work_available = self.get_routing_work_available();
            let work_needed = self.get_routing_work_needed(previous_block, current_timestamp);
            debug!(
                "last ts: {:?}, this ts: {:?}, work available: {:?}, work needed: {:?}",
                previous_block.timestamp, current_timestamp, work_available, work_needed
            );
            let time_elapsed = current_timestamp - previous_block.timestamp;
            debug!(
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

    #[tracing::instrument(level = "info", skip_all)]
    pub fn delete_block(&mut self, block_hash: &SaitoHash) {
        debug!(
            "deleting block from mempool : {:?}",
            hex::encode(block_hash)
        );

        // self.blocks_queue.retain(|block| !block.hash.eq(block_hash));
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn delete_transactions(&mut self, transactions: &Vec<Transaction>) {
        let mut tx_hashmap = HashMap::new();
        for transaction in transactions {
            let hash = transaction.hash_for_signature;
            tx_hashmap.entry(hash).or_insert(true);
        }

        self.routing_work_in_mempool = 0;

        self.transactions
            .retain(|x| tx_hashmap.contains_key(&x.hash_for_signature) != true);

        // add routing work from remaining tx
        for transaction in &self.transactions {
            self.routing_work_in_mempool += transaction.total_work;
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
    #[tracing::instrument(level = "info", skip_all)]
    pub fn get_routing_work_needed(&self, previous_block: &Block, current_timestamp: u64) -> u64 {
        let previous_block_timestamp = previous_block.timestamp;
        let previous_block_burnfee = previous_block.burnfee;

        let work_needed: u64 = BurnFee::return_routing_work_needed_to_produce_block_in_nolan(
            previous_block_burnfee,
            current_timestamp,
            previous_block_timestamp,
        );

        work_needed
    }

    pub fn set_mempool_public_key(&mut self, public_key: SaitoPublicKey) {
        self.mempool_public_key = public_key;
    }

    pub fn set_mempool_private_key(&mut self, private_key: SaitoPrivateKey) {
        self.mempool_private_key = private_key;
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn transaction_exists(&self, tx_hash: Option<SaitoHash>) -> bool {
        self.transactions
            .iter()
            .any(|transaction| transaction.hash_for_signature == tx_hash)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::common::test_manager::test::{create_timestamp, TestManager};
    use crate::core::data::burnfee::HEARTBEAT;

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

    #[tokio::test]
    #[serial_test::serial]
    async fn mempool_bundle_blocks_test() {
        let mempool_lock: Arc<RwLock<Mempool>>;
        let wallet_lock: Arc<RwLock<Wallet>>;
        let blockchain_lock: Arc<RwLock<Blockchain>>;
        let public_key: SaitoPublicKey;
        let private_key: SaitoPrivateKey;

        {
            let mut t = TestManager::new();
            t.initialize(100, 720_000).await;
            t.wait_for_mining_event().await;

            wallet_lock = t.get_wallet_lock();
            mempool_lock = t.get_mempool_lock();
            blockchain_lock = t.get_blockchain_lock();
        }

        {
            let wallet = wallet_lock.write().await;
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let ts = create_timestamp();
        let _next_block_timestamp = ts + (HEARTBEAT * 2);

        let mut mempool = mempool_lock.write().await;
        let _txs = Vec::<Transaction>::new();

        assert_eq!(mempool.get_routing_work_available(), 0);

        for _i in 0..5 {
            let mut tx = Transaction::new();

            {
                let mut wallet = wallet_lock.write().await;
                let (inputs, outputs) = wallet.generate_slips(720_000);
                tx.inputs = inputs;
                tx.outputs = outputs;
                // _i prevents sig from being identical during test
                // and thus from being auto-rejected from mempool
                tx.timestamp = ts + 120000 + _i;
                tx.generate(public_key);
                tx.sign(private_key);
            }
            let wallet = wallet_lock.read().await;
            tx.add_hop(&wallet, public_key);

            mempool.add_transaction(tx).await;
        }

        assert_eq!(mempool.transactions.len(), 5);
        assert_eq!(mempool.get_routing_work_available(), 3_600_000);

        // TODO : FIX THIS TEST
        // assert_eq!(
        //     mempool.can_bundle_block(blockchain_lock.clone(), ts).await,
        //     false
        // );
        let blockchain = blockchain_lock.read().await;
        assert_eq!(
            mempool.can_bundle_block(&blockchain, ts + 120000).await,
            true
        );
    }
}

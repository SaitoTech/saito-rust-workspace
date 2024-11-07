use pyo3::pyclass;
use std::sync::Arc;
use tokio::sync::RwLock;

use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::defs::{BlockId, PrintForLog, SaitoHash};

#[pyclass]
#[derive(Clone)]
pub struct WasmBlockchain {
    pub(crate) blockchain_lock: Arc<RwLock<Blockchain>>,
}

impl WasmBlockchain {
    pub async fn reset(&self) {
        let mut blockchain = self.blockchain_lock.write().await;
        blockchain.reset().await;
        blockchain.save().await;
    }

    pub async fn get_last_block_id(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.last_block_id
    }
    pub async fn get_last_timestamp(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.last_timestamp
    }
    pub async fn get_longest_chain_hash_at(&self, id: BlockId) -> String {
        let blockchain = self.blockchain_lock.read().await;
        let hash = blockchain
            .blockring
            .get_longest_chain_block_hash_at_block_id(id);
        hash.to_hex().into()
    }
    pub async fn get_last_block_hash(&self) -> String {
        let blockchain = self.blockchain_lock.read().await;
        let hash = blockchain.last_block_hash;
        hash.to_hex().into()
    }
    pub async fn get_last_burnfee(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.last_burnfee
    }
    pub async fn get_genesis_block_id(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.genesis_block_id
    }
    pub async fn get_genesis_timestamp(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.genesis_timestamp
    }
    pub async fn get_lowest_acceptable_timestamp(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.lowest_acceptable_timestamp
    }
    pub async fn get_lowest_acceptable_block_hash(&self) -> String {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.lowest_acceptable_block_hash.to_hex().into()
    }
    pub async fn get_lowest_acceptable_block_id(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.lowest_acceptable_block_id
    }
    pub async fn get_latest_block_id(&self) -> u64 {
        let blockchain = self.blockchain_lock.read().await;
        return blockchain.get_latest_block_id();
    }

    pub async fn get_fork_id(&self) -> String {
        let blockchain = self.blockchain_lock.read().await;
        blockchain.fork_id.to_hex().into()
    }

    pub async fn get_longest_chain_hash_at_id(&self, block_id: u64) -> String {
        let blockchain = self.blockchain_lock.read().await;
        let hash = blockchain
            .blockring
            .get_longest_chain_block_hash_at_block_id(block_id);
        hash.to_hex().into()
    }
    pub async fn get_hashes_at_id(&self, block_id: u64) -> Vec<SaitoHash> {
        let blockchain = self.blockchain_lock.read().await;
        let hashes = blockchain.blockring.get_block_hashes_at_block_id(block_id);

        hashes
    }
}

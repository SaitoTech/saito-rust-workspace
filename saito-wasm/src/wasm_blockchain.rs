use js_sys::JsString;
use std::sync::Arc;

use saito_core::common::defs::BlockId;
use tokio::sync::RwLock;
use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::core::data::blockchain::Blockchain;

#[wasm_bindgen]
#[derive(Clone)]
pub struct WasmBlockchain {
    pub(crate) blockchain: Arc<RwLock<Blockchain>>,
}

#[wasm_bindgen]
impl WasmBlockchain {
    pub async fn reset(&self) {
        let mut blockchain = self.blockchain.write().await;
        blockchain.reset().await;
        blockchain.save().await;
    }

    pub async fn get_last_block_id(&self) -> u64 {
        let blockchain = self.blockchain.read().await;
        blockchain.last_block_id
    }
    pub async fn get_last_timestamp(&self) -> u64 {
        let blockchain = self.blockchain.read().await;
        blockchain.last_timestamp
    }
    pub async fn get_longest_chain_hash_at(&self, id: BlockId) -> JsString {
        let blockchain = self.blockchain.read().await;
        let hash = blockchain
            .blockring
            .get_longest_chain_block_hash_by_block_id(id);
        hex::encode(hash).into()
    }
}

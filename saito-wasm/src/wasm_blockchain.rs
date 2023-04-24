use std::sync::Arc;

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
}

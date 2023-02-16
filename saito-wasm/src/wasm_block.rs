use js_sys::{Array, JsString};
use num_traits::FromPrimitive;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::core::data::block::{Block, BlockType};

use crate::saitowasm::string_to_key;
use crate::wasm_transaction::WasmTransaction;

#[wasm_bindgen]
pub struct WasmBlock {
    block: Block,
}

#[wasm_bindgen]
impl WasmBlock {
    #[wasm_bindgen(getter = transactions)]
    pub fn get_transactions(&self) -> Array {
        let mut txs: Vec<WasmTransaction> = self
            .block
            .transactions
            .iter()
            .map(|tx| WasmTransaction::from_transaction(tx.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(txs.len() as u32);
        for (i, tx) in txs.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(tx));
        }
        array
    }
    #[wasm_bindgen(getter = id)]
    pub fn get_id(&self) -> u64 {
        self.block.id
    }
    #[wasm_bindgen(setter = id)]
    pub fn set_id(&mut self, id: u64) {
        self.block.id = id;
    }
    #[wasm_bindgen(getter = timestamp)]
    pub fn get_timestamp(&self) -> u64 {
        self.block.timestamp
    }
    #[wasm_bindgen(setter = timestamp)]
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.block.timestamp = timestamp;
    }
    #[wasm_bindgen(getter = previous_block_hash)]
    pub fn get_previous_block_hash(&self) -> JsString {
        hex::encode(self.block.previous_block_hash).into()
    }
    #[wasm_bindgen(setter = previous_block_hash)]
    pub fn set_previous_block_hash(&mut self, hash: JsString) {
        self.block.previous_block_hash = string_to_key(hash).unwrap();
    }
    #[wasm_bindgen(setter = creator)]
    pub fn set_creator(&mut self, key: JsString) {
        self.block.creator = string_to_key(key).unwrap();
    }
    #[wasm_bindgen(getter = creator)]
    pub fn get_creator(&self) -> JsString {
        hex::encode(self.block.creator).into()
    }
    #[wasm_bindgen(getter = type)]
    pub fn get_type(&self) -> u8 {
        self.block.block_type as u8
    }
    #[wasm_bindgen(setter = type)]
    pub fn set_type(&mut self, t: u8) {
        self.block.block_type = BlockType::from_u8(t).unwrap();
    }
    #[wasm_bindgen(getter = hash)]
    pub fn get_hash(&self) -> JsString {
        hex::encode(self.block.hash).into()
    }
}

impl WasmBlock {
    pub fn from_block(block: Block) -> WasmBlock {
        WasmBlock { block }
    }
}

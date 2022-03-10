use std::sync::RwLock;

use js_sys::{BigInt, Uint8Array};
use wasm_bindgen::prelude::*;

use saito_core::common::defs::{Currency, Signature};
use saito_core::saito::Saito;

#[wasm_bindgen]
pub struct SaitoWasm {
    saito: Saito,
    lock: RwLock<u8>,
}

#[wasm_bindgen]
pub struct WasmSlip {}

#[wasm_bindgen]
pub struct WasmTransaction {
    from: Vec<WasmSlip>,
    to: Vec<WasmSlip>,
    fees_total: Currency,
    timestamp: u64,
    signature: Signature,
}

impl WasmTransaction {
    pub fn add_to_slip(&mut self, slip: WasmSlip) {}
    pub fn add_from_slip(&mut self, slip: WasmSlip) {}
}

#[wasm_bindgen]
impl SaitoWasm {
    pub fn send_transaction(&self, transaction: WasmTransaction) -> Result<JsValue, JsValue> {
        todo!()
    }
}

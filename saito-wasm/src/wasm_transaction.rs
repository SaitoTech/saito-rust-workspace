use js_sys::Array;
use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::common::defs::{Currency, SaitoSignature};
use saito_core::core::data::transaction::Transaction;

use crate::wasm_slip::WasmSlip;

#[wasm_bindgen]
pub struct WasmTransaction {
    pub(crate) from: Vec<WasmSlip>,
    pub(crate) to: Vec<WasmSlip>,
    pub(crate) fees_total: Currency,
    pub timestamp: u64,
    pub(crate) signature: SaitoSignature,
}

#[wasm_bindgen]
impl WasmTransaction {
    pub fn new() -> WasmTransaction {
        WasmTransaction {
            from: vec![],
            to: vec![],
            fees_total: 0,
            timestamp: 0,
            signature: [0; 64],
        }
    }
    pub fn add_to_slip(&mut self, slip: WasmSlip) {}
    pub fn add_from_slip(&mut self, slip: WasmSlip) {}
    pub fn get_to_slips(&self) -> Array {
        todo!()
    }
    pub fn get_from_slips(&self) -> Array {
        todo!()
    }
    pub(crate) fn from_transaction(transaction: Transaction) -> WasmTransaction {
        todo!()
    }
}

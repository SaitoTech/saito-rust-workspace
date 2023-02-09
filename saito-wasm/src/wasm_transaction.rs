use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::common::defs::{Currency, SaitoSignature};
use saito_core::core::data::transaction::Transaction;

use crate::wasm_slip::WasmSlip;

#[wasm_bindgen]
pub struct WasmTransaction {
    tx: Transaction,
}

#[wasm_bindgen]
impl WasmTransaction {
    pub fn new() -> WasmTransaction {
        WasmTransaction {
            tx: Transaction::default(),
        }
    }
    pub fn get_signature(&self) -> js_sys::Uint8Array {
        let buffer = Uint8Array::new_with_length(64);
        buffer.copy_from(self.tx.signature.as_slice());

        buffer
    }
    pub fn add_to_slip(&mut self, slip: WasmSlip) {}
    pub fn add_from_slip(&mut self, slip: WasmSlip) {}
    pub fn get_to_slips(&self) -> Array {
        todo!()
    }

    pub fn get_from_slips(&self) -> Array {
        todo!()
    }

    pub fn get_data(&self) -> js_sys::Uint8Array {
        let buffer = js_sys::Uint8Array::new_with_length(self.tx.message.len() as u32);
        buffer.copy_from(self.tx.message.as_slice());

        buffer
    }
    pub fn sign(&mut self) {}

    pub fn sign_and_encrypt(&mut self) {}
}

impl WasmTransaction {
    pub fn from_transaction(transaction: Transaction) -> WasmTransaction {
        WasmTransaction { tx: transaction }
    }
}

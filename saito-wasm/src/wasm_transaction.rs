use js_sys::{Array, JsString, Uint8Array};
use log::error;
use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::common::defs::{Currency, SaitoPrivateKey, SaitoSignature};
use saito_core::core::data::transaction::Transaction;

use crate::saitowasm::{string_to_key, SAITO};
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
    pub async fn sign(&mut self) {
        let saito = SAITO.lock().await;
        let wallet = saito.context.wallet.read().await;

        self.tx.sign(&wallet.private_key);
    }

    pub async fn sign_and_encrypt(&mut self) {
        let saito = SAITO.lock().await;
        let wallet = saito.context.wallet.read().await;

        self.tx.sign_and_encrypt(&wallet.private_key);
    }
}

impl WasmTransaction {
    pub fn from_transaction(transaction: Transaction) -> WasmTransaction {
        WasmTransaction { tx: transaction }
    }
}

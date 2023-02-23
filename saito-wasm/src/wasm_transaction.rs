use js_sys::{Array, JsString, Uint8Array};
use log::error;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::{Currency, SaitoPrivateKey, SaitoSignature};
use saito_core::core::data::transaction::{Transaction, TransactionType};

use crate::saitowasm::{string_to_key, SAITO};
use crate::wasm_slip::WasmSlip;

#[wasm_bindgen]
pub struct WasmTransaction {
    pub(crate) tx: Transaction,
}

#[wasm_bindgen]
impl WasmTransaction {
    pub fn new() -> WasmTransaction {
        WasmTransaction {
            tx: Transaction::default(),
        }
    }
    #[wasm_bindgen(getter = signature)]
    pub fn signature(&self) -> js_sys::JsString {
        hex::encode(self.tx.signature).into()
    }
    #[wasm_bindgen(setter = signature)]
    pub fn set_signature(&mut self, signature: JsString) {
        self.tx.signature = string_to_key(signature).unwrap();
    }

    pub fn add_to_slip(&mut self, slip: WasmSlip) {
        self.tx.add_to_slip(slip.slip);
    }

    pub fn add_from_slip(&mut self, slip: WasmSlip) {
        self.tx.add_from_slip(slip.slip);
    }

    #[wasm_bindgen(getter = txs_replacements)]
    pub fn get_txs_replacements(&self) -> u32 {
        self.tx.txs_replacements
    }
    #[wasm_bindgen(setter = txs_replacements)]
    pub fn set_txs_replacements(&mut self, r: u32) {
        self.tx.txs_replacements = r;
    }

    #[wasm_bindgen(getter)]
    pub fn output_slips(&self) -> Array {
        let mut slips: Vec<WasmSlip> = self
            .tx
            .to
            .iter()
            .map(|slip| WasmSlip::new_from_slip(slip.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(slips.len() as u32);
        for (i, slip) in slips.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(slip));
        }
        array
    }

    #[wasm_bindgen(getter)]
    pub fn input_slips(&self) -> Array {
        let mut slips: Vec<WasmSlip> = self
            .tx
            .from
            .iter()
            .map(|slip| WasmSlip::new_from_slip(slip.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(slips.len() as u32);
        for (i, slip) in slips.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(slip));
        }
        array
    }

    #[wasm_bindgen(getter = data)]
    pub fn get_data(&self) -> js_sys::Uint8Array {
        let buffer = js_sys::Uint8Array::new_with_length(self.tx.data.len() as u32);
        buffer.copy_from(self.tx.data.as_slice());

        buffer
    }
    #[wasm_bindgen(setter = data)]
    pub fn set_data(&mut self, buffer: Uint8Array) {
        self.tx.data = buffer.to_vec();
    }

    #[wasm_bindgen(getter = timestamp)]
    pub fn get_timestamp(&self) -> u64 {
        self.tx.timestamp
    }
    #[wasm_bindgen(setter = timestamp)]
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.tx.timestamp = timestamp;
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
    #[wasm_bindgen(getter = type)]
    pub fn get_type(&self) -> u8 {
        self.tx.transaction_type as u8
    }
    #[wasm_bindgen(setter = type)]
    pub fn set_type(&mut self, t: u8) {
        self.tx.transaction_type =
            TransactionType::from_u8(t).expect("invalid value for transaction type");
    }
    #[wasm_bindgen(getter=total_fees)]
    pub fn total_fees(&self) -> Currency {
        self.tx.total_fees
    }
}

impl WasmTransaction {
    pub fn from_transaction(transaction: Transaction) -> WasmTransaction {
        WasmTransaction { tx: transaction }
    }
}

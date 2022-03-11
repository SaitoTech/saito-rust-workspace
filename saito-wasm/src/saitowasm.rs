use std::sync::RwLock;

use js_sys::{Array, BigInt, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use saito_core::common::defs::{Currency, Hash32, PublicKey, Signature};
use saito_core::saito::Saito;

use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_task_runner::WasmTaskRunner;

#[wasm_bindgen]
pub struct SaitoWasm {
    saito: Saito<WasmIoHandler, WasmTaskRunner>,
    lock: RwLock<u8>,
}

// #[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct WasmSlip {
    pub(crate) public_key: PublicKey,
    pub(crate) uuid: Hash32,
    pub(crate) amount: Currency,
}

#[wasm_bindgen]
impl WasmSlip {
    pub fn new() -> WasmSlip {
        WasmSlip {
            public_key: [0; 33],
            uuid: [0; 32],
            amount: 0,
        }
    }
}

#[wasm_bindgen]
pub struct WasmTransaction {
    pub(crate) from: Vec<WasmSlip>,
    pub(crate) to: Vec<WasmSlip>,
    pub(crate) fees_total: Currency,
    pub timestamp: u64,
    pub(crate) signature: Signature,
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
}

#[wasm_bindgen]
impl SaitoWasm {
    pub fn send_transaction(&mut self, transaction: WasmTransaction) -> Result<JsValue, JsValue> {
        // todo : convert transaction
        self.saito.process_message_buffer(0, [0, 1, 2, 3].to_vec());

        Ok(JsValue::from("test"))
    }
}

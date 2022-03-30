use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::common::defs::{Currency, SaitoHash, SaitoPublicKey};

// #[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct WasmSlip {
    pub(crate) public_key: SaitoPublicKey,
    pub(crate) uuid: SaitoHash,
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

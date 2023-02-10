use wasm_bindgen::prelude::wasm_bindgen;

use saito_core::common::defs::{Currency, SaitoHash, SaitoPublicKey};
use saito_core::core::data::slip::Slip;

// #[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct WasmSlip {
    slip: Slip,
}

#[wasm_bindgen]
impl WasmSlip {
    pub fn get_amount(&self) -> u64 {
        self.slip.amount
    }
}

impl WasmSlip {
    pub fn new(slip: Slip) -> WasmSlip {
        WasmSlip { slip }
    }
}

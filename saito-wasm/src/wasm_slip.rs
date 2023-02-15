use js_sys::{JsString, Uint8Array};
use wasm_bindgen::prelude::wasm_bindgen;

use num_traits::FromPrimitive;
use saito_core::common::defs::{Currency, SaitoHash, SaitoPublicKey, SaitoUTXOSetKey};
use saito_core::core::data::slip::{Slip, SlipType};

use crate::saitowasm::string_to_key;

// #[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct WasmSlip {
    pub(crate) slip: Slip,
}

#[wasm_bindgen]
impl WasmSlip {
    #[wasm_bindgen(getter=amount)]
    pub fn amount(&self) -> u64 {
        self.slip.amount
    }
    #[wasm_bindgen(setter=amount)]
    pub fn set_amount(&mut self, amount: Currency) {
        self.slip.amount = amount;
    }
    #[wasm_bindgen(getter=slip_type)]
    pub fn slip_type(&self) -> u8 {
        self.slip.slip_type as u8
    }
    #[wasm_bindgen(setter=slip_type)]
    pub fn set_slip_type(&mut self, slip_type: u8) {
        self.slip.slip_type = SlipType::from_u8(slip_type).expect("value is not in slip types");
    }
    #[wasm_bindgen(getter=public_key)]
    pub fn public_key(&self) -> JsString {
        let key = hex::encode(self.slip.public_key);
        key.into()
    }
    #[wasm_bindgen(setter=public_key)]
    pub fn set_public_key(&mut self, key: JsString) {
        let key = string_to_key(key).unwrap();

        self.slip.public_key = key;
    }
    #[wasm_bindgen(getter=slip_index)]
    pub fn slip_index(&self) -> u8 {
        self.slip.slip_index
    }
    #[wasm_bindgen(setter=slip_index)]
    pub fn set_slip_index(&mut self, index: u8) {
        self.slip.slip_index = index;
    }
    #[wasm_bindgen(getter=block_id)]
    pub fn block_id(&self) -> u64 {
        self.slip.block_id
    }
    #[wasm_bindgen(setter=block_id)]
    pub fn set_block_id(&mut self, id: u64) {
        self.slip.block_id = id;
    }
    #[wasm_bindgen(getter=tx_ordinal)]
    pub fn tx_ordinal(&self) -> u64 {
        self.slip.tx_ordinal
    }
    #[wasm_bindgen(setter=tx_ordinal)]
    pub fn set_tx_ordinal(&mut self, ordinal: u64) {
        self.slip.tx_ordinal = ordinal;
    }
    #[wasm_bindgen(setter=utxo_key)]
    pub fn set_utxo_key(&mut self, key: JsString) {
        let key: SaitoUTXOSetKey = string_to_key(key).unwrap();
        self.slip.utxoset_key = key;
    }
    #[wasm_bindgen(getter=utxo_key)]
    pub fn utxo_key(&self) -> JsString {
        let key = hex::encode(self.slip.utxoset_key);
        key.into()
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmSlip {
        WasmSlip {
            slip: Slip {
                public_key: [0; 33],
                amount: 0,
                slip_index: 0,
                block_id: 0,
                tx_ordinal: 0,
                slip_type: SlipType::Normal,
                utxoset_key: [0; 58],
                is_utxoset_key_set: false,
            },
        }
    }
}

impl WasmSlip {
    pub fn new_from_slip(slip: Slip) -> WasmSlip {
        WasmSlip { slip }
    }
}

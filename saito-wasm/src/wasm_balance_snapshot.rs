use js_sys::{Array, JsString};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::core::util::balance_snapshot::BalanceSnapshot;

#[wasm_bindgen]
pub struct WasmBalanceSnapshot {
    snapshot: BalanceSnapshot,
}

impl WasmBalanceSnapshot {
    pub fn new(snapshot: BalanceSnapshot) -> WasmBalanceSnapshot {
        WasmBalanceSnapshot { snapshot }
    }
}

#[wasm_bindgen]
impl WasmBalanceSnapshot {
    pub fn get_file_name(&self) -> JsString {
        self.snapshot.get_file_name().into()
    }
    pub fn get_entries(&self) -> Array {
        let rows = self.snapshot.get_rows();
        let array = js_sys::Array::new_with_length(rows.len() as u32);
        for (index, row) in rows.iter().enumerate() {
            let entry: JsString = row.to_string().into();
            array.set(index as u32, JsValue::from(entry));
        }
        array
    }
}

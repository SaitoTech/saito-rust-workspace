use js_sys::Object;
use saito_core::{common::defs::PrintForLog, core::data::hop::Hop};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmHop {
    pub(crate) hop: Hop,
}

impl WasmHop {
    pub fn from_hop(hop: Hop) -> WasmHop {
        WasmHop { hop: hop }
    }
}

#[wasm_bindgen]
impl WasmHop {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmHop {
        WasmHop {
            hop: Hop::default(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn from(&self) -> String {
        self.hop.from.to_base58()
    }
    #[wasm_bindgen(getter)]
    pub fn sig(&self) -> String {
        self.hop.to.to_base58()
    }
    #[wasm_bindgen(getter)]
    pub fn to(&self) -> String {
        self.hop.sig.to_base58()
    }

    pub fn to_js_object(&self) -> Result<Object, String> {
        let obj = Object::new();
        js_sys::Reflect::set(&obj, &"from".into(), &self.from().into())
            .map_err(|_| "Error setting 'from' property".to_string())?;
        js_sys::Reflect::set(&obj, &"to".into(), &self.to().into())
            .map_err(|_| "Error setting 'to' property".to_string())?;
        js_sys::Reflect::set(&obj, &"sig".into(), &self.sig().into())
            .map_err(|_| "Error setting 'sig' property".to_string())?;
        Ok(obj)
    }
}

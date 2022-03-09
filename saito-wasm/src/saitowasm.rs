use std::sync::RwLock;

use wasm_bindgen::prelude::*;

use saito_core::saito::Saito;

#[wasm_bindgen]
pub struct SaitoWasm {
    saito: Saito,
    lock: RwLock<u8>,
}

impl SaitoWasm {}

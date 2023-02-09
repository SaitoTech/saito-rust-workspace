use saito_core::core::data::block::Block;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct WasmBlock {
    block: Block,
}

#[wasm_bindgen]
impl WasmBlock {}

impl WasmBlock {
    pub fn from_block(block: Block) -> WasmBlock {
        WasmBlock { block }
    }
}

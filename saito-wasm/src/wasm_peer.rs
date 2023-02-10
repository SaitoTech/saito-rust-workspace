use saito_core::core::data::peer::Peer;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct WasmPeer {
    peer: Peer,
}

#[wasm_bindgen]
impl WasmPeer {}

impl WasmPeer {
    pub fn new(peer: Peer) -> WasmPeer {
        WasmPeer { peer }
    }
}

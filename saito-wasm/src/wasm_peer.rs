use js_sys::JsString;
use saito_core::core::data::peer::Peer;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct WasmPeer {
    peer: Peer,
}

#[wasm_bindgen]
impl WasmPeer {
    #[wasm_bindgen(getter=public_key)]
    pub fn get_public_key(&self) -> JsString {
        hex::encode(self.peer.public_key.unwrap()).into()
    }
}

impl WasmPeer {
    pub fn new(peer: Peer) -> WasmPeer {
        WasmPeer { peer }
    }
}

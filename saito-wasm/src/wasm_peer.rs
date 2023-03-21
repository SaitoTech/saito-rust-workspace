use js_sys::{Array, JsString};
use log::warn;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::PeerIndex;
use saito_core::core::data::peer::Peer;

#[wasm_bindgen]
pub struct WasmPeer {
    peer: Peer,
}

#[wasm_bindgen]
impl WasmPeer {
    #[wasm_bindgen(getter = public_key)]
    pub fn get_public_key(&self) -> JsString {
        if self.peer.public_key.is_none() {
            warn!("peer : {:?} public key is not set", self.peer.index);
        }
        hex::encode(self.peer.public_key.unwrap()).into()
    }
    #[wasm_bindgen(getter = key_list)]
    pub fn get_key_list(&self) -> Array {
        let array = Array::new_with_length(self.peer.key_list.len() as u32);
        for (i, key) in self.peer.key_list.iter().enumerate() {
            array.set(i as u32, JsValue::from(hex::encode(key)));
        }
        array
    }

    #[wasm_bindgen(getter = peer_index)]
    pub fn get_peer_index(&self) -> u64 {
        self.peer.index
    }
    #[wasm_bindgen(constructor)]
    pub fn new(peer_index: PeerIndex) -> WasmPeer {
        WasmPeer {
            peer: Peer::new(peer_index),
        }
    }
}

impl WasmPeer {
    pub fn new_from_peer(peer: Peer) -> WasmPeer {
        WasmPeer { peer }
    }
}

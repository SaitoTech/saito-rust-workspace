use js_sys::{Array, JsString};
use log::{info, warn};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::PeerIndex;
use saito_core::core::data::peer::Peer;

#[wasm_bindgen]
#[derive(Clone)]
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
    #[wasm_bindgen(getter = sync_type)]
    pub fn get_sync_type(&self) -> JsString {
        if self.peer.block_fetch_url.is_empty() {
            return "lite".into();
        }
        return "full".into();
    }
    #[wasm_bindgen(getter = services)]
    pub fn get_services(&self) -> JsValue {
        let arr = js_sys::Array::new_with_length(self.peer.services.len() as u32);
        for (i, service) in self.peer.services.iter().enumerate() {
            arr.set(i as u32, JsValue::from(JsString::from(service.as_str())));
        }
        JsValue::from(arr)
    }
    #[wasm_bindgen(setter = services)]
    pub fn set_services(&mut self, services: JsValue) {
        let services = js_sys::Array::from(&services);
        let mut ser = vec![];
        for i in 0..services.length() {
            let str = JsString::from(services.at(i as i32));
            ser.push(str.into());
        }
        self.peer.services = ser;
    }
    pub fn has_service(&self, service: JsString) -> bool {
        return self.peer.has_service(service.into());
    }
}

impl WasmPeer {
    pub fn new_from_peer(peer: Peer) -> WasmPeer {
        WasmPeer { peer }
    }
}

use log::warn;
use pyo3::pyclass;

use saito_core::core::consensus::peers::peer::Peer;
use saito_core::core::consensus::peers::peer_service::PeerService;
use saito_core::core::defs::{PeerIndex, PrintForLog, SaitoPublicKey};

#[pyclass]
#[derive(Clone)]
pub struct PyPeer {
    peer: Peer,
}

impl PyPeer {
    pub fn get_public_key(&self) -> String {
        if self.peer.get_public_key().is_none() {
            warn!("peer : {:?} public key is not set", self.peer.index);
            return String::from([0; 33].to_base58());
        }
        self.peer.get_public_key().unwrap().to_base58().into()
    }
    pub fn get_key_list(&self) -> Vec<SaitoPublicKey> {
        self.peer.key_list.clone()
    }

    pub fn get_peer_index(&self) -> u64 {
        self.peer.index
    }
    pub fn new(peer_index: PeerIndex) -> PyPeer {
        PyPeer {
            peer: Peer::new(peer_index),
        }
    }
    pub fn get_sync_type(&self) -> String {
        if self.peer.block_fetch_url.is_empty() {
            return "lite".into();
        }
        return "full".into();
    }
    pub fn get_services(&self) -> Vec<PeerService> {
        self.peer.services.clone()
    }
    // pub fn set_services(&mut self, services: JsValue) {
    //     let mut services: Vec<WasmPeerService> = serde_wasm_bindgen::from_value(services).unwrap();
    //     let services = services.drain(..).map(|s| s.service).collect();
    //
    //     // let mut ser = vec![];
    //     // for i in 0..services.length() {
    //     //     let str = WasmPeerService::from(services.at(i as i32));
    //     //     ser.push(str.service);
    //     // }
    //     self.peer.services = services;
    // }
    pub fn has_service(&self, service: String) -> bool {
        self.peer.has_service(service.into())
    }
}

impl PyPeer {
    pub fn new_from_peer(peer: Peer) -> PyPeer {
        PyPeer { peer }
    }
}

use pyo3::pyclass;
use saito_core::core::consensus::peers::peer_service::PeerService;
use serde::{Deserialize, Serialize};

#[pyclass]
pub struct WasmPeerServiceList {
    pub(crate) services: Vec<WasmPeerService>,
}

#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct WasmPeerService {
    pub(crate) service: PeerService,
}

impl WasmPeerService {
    pub fn new() -> WasmPeerService {
        WasmPeerService {
            service: PeerService {
                service: "".to_string(),
                domain: "".to_string(),
                name: "".to_string(),
            },
        }
    }
    pub fn set_service(&mut self, value: String) {
        self.service.service = value.into();
    }

    pub fn get_service(&self) -> String {
        self.service.service.clone().into()
    }

    pub fn set_name(&mut self, value: String) {
        self.service.name = value.into();
    }
    pub fn get_name(&self) -> String {
        self.service.name.clone().into()
    }
    pub fn set_domain(&mut self, value: String) {
        self.service.domain = value.into();
    }
    pub fn get_domain(&self) -> String {
        self.service.domain.clone().into()
    }
}

impl WasmPeerServiceList {
    pub fn push(&mut self, service: WasmPeerService) {
        self.services.push(service);
    }
    pub fn new() -> WasmPeerServiceList {
        WasmPeerServiceList { services: vec![] }
    }
}

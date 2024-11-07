use pyo3::pyclass;
use saito_core::{core::consensus::hop::Hop, core::defs::PrintForLog};

#[pyclass]
pub struct WasmHop {
    pub(crate) hop: Hop,
}

impl WasmHop {
    pub fn from_hop(hop: Hop) -> WasmHop {
        WasmHop { hop: hop }
    }
}

impl WasmHop {
    pub fn from(&self) -> String {
        self.hop.from.to_base58()
    }
    pub fn sig(&self) -> String {
        self.hop.to.to_base58()
    }
    pub fn to(&self) -> String {
        self.hop.sig.to_base58()
    }
}

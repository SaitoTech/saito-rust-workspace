use pyo3::pyclass;
use saito_core::{core::consensus::hop::Hop, core::defs::PrintForLog};

#[pyclass]
pub struct PyHop {
    pub(crate) hop: Hop,
}

impl PyHop {
    pub fn from_hop(hop: Hop) -> PyHop {
        PyHop { hop: hop }
    }
}

impl PyHop {
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

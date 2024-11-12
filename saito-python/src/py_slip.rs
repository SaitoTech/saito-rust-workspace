use log::warn;
use pyo3::pyclass;

use num_traits::FromPrimitive;
use saito_core::core::consensus::slip::{Slip, SlipType};
use saito_core::core::defs::{Currency, PrintForLog, SaitoUTXOSetKey, UTXO_KEY_LENGTH};

use crate::saitopython::{string_to_hex, string_to_key};

// #[derive(Serialize, Deserialize)]
#[pyclass]
pub struct PySlip {
    pub(crate) slip: Slip,
}

impl PySlip {
    pub fn amount(&self) -> u64 {
        self.slip.amount
    }
    pub fn set_amount(&mut self, amount: Currency) {
        self.slip.amount = amount;
    }
    pub fn slip_type(&self) -> u8 {
        self.slip.slip_type as u8
    }
    pub fn set_slip_type(&mut self, slip_type: u8) {
        self.slip.slip_type = SlipType::from_u8(slip_type).expect("value is not in slip types");
    }
    pub fn public_key(&self) -> String {
        let key = self.slip.public_key.to_base58();
        key.into()
    }
    pub fn set_public_key(&mut self, key: String) {
        let key2 = string_to_key(key);
        if key2.is_err() {
            warn!("cannot parse key . {:?}", key2.err().unwrap());
            return;
        }
        self.slip.public_key = key2.unwrap();
    }
    pub fn slip_index(&self) -> u8 {
        self.slip.slip_index
    }
    pub fn set_slip_index(&mut self, index: u8) {
        self.slip.slip_index = index;
    }
    pub fn block_id(&self) -> u64 {
        self.slip.block_id
    }
    pub fn set_block_id(&mut self, id: u64) {
        self.slip.block_id = id;
    }
    pub fn tx_ordinal(&self) -> u64 {
        self.slip.tx_ordinal
    }
    pub fn set_tx_ordinal(&mut self, ordinal: u64) {
        self.slip.tx_ordinal = ordinal;
    }
    pub fn set_utxo_key(&mut self, key: String) {
        let key: SaitoUTXOSetKey = string_to_hex(key).unwrap();
        self.slip.utxoset_key = key;
    }
    pub fn utxo_key(&self) -> String {
        let key = self.slip.utxoset_key.to_hex();
        key.into()
    }

    pub fn new() -> PySlip {
        PySlip {
            slip: Slip {
                public_key: [0; 33],
                amount: 0,
                slip_index: 0,
                block_id: 0,
                tx_ordinal: 0,
                slip_type: SlipType::Normal,
                utxoset_key: [0; UTXO_KEY_LENGTH],
                is_utxoset_key_set: false,
            },
        }
    }
}

impl PySlip {
    pub fn new_from_slip(slip: Slip) -> PySlip {
        PySlip { slip }
    }
}

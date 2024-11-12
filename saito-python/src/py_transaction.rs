use num_traits::FromPrimitive;
use pyo3::pyclass;

use saito_core::core::consensus::transaction::{Transaction, TransactionType};
use saito_core::core::defs::{Currency, PrintForLog, Timestamp};

use crate::py_slip::WasmSlip;
use crate::saitopython::{string_to_hex, string_to_key, SAITO};

#[pyclass]
#[derive(Clone)]
pub struct WasmTransaction {
    pub(crate) tx: Transaction,
}

impl WasmTransaction {
    pub fn new() -> WasmTransaction {
        WasmTransaction {
            tx: Transaction::default(),
        }
    }
    pub fn signature(&self) -> String {
        self.tx.signature.to_hex().into()
    }

    pub fn set_signature(&mut self, signature: String) {
        self.tx.signature = string_to_hex(signature).unwrap();
    }

    pub fn add_to_slip(&mut self, slip: WasmSlip) {
        self.tx.add_to_slip(slip.slip.clone());
    }

    pub fn add_from_slip(&mut self, slip: WasmSlip) {
        self.tx.add_from_slip(slip.slip.clone());
    }

    pub fn get_txs_replacements(&self) -> u32 {
        self.tx.txs_replacements
    }
    pub fn set_txs_replacements(&mut self, r: u32) {
        self.tx.txs_replacements = r;
    }

    pub fn is_from(&self, key: String) -> bool {
        let key = string_to_key(key);
        if key.is_err() {
            return false;
        }
        return self.tx.is_from(&key.unwrap());
    }
    pub fn is_to(&self, key: String) -> bool {
        let key = string_to_key(key);
        if key.is_err() {
            return false;
        }
        return self.tx.is_to(&key.unwrap());
    }

    pub fn get_timestamp(&self) -> Timestamp {
        self.tx.timestamp
    }
    pub fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.tx.timestamp = timestamp;
    }

    pub async fn sign(&mut self) {
        let saito = SAITO.lock().await;
        let wallet = saito.as_ref().unwrap().context.wallet_lock.read().await;
        self.tx.sign(&wallet.private_key);
    }

    pub fn get_type(&self) -> u8 {
        self.tx.transaction_type as u8
    }
    pub fn set_type(&mut self, t: u8) {
        self.tx.transaction_type =
            TransactionType::from_u8(t).expect("invalid value for transaction type");
    }
    pub fn total_fees(&self) -> Currency {
        self.tx.total_fees
    }
    // pub fn serialize(&self) -> Uint8Array {
    //     let buffer = self.tx.serialize_for_net();
    //     let res = Uint8Array::new_with_length(buffer.len() as u32);
    //     res.copy_from(buffer.as_slice());
    //     return res;
    // }
    // pub fn deserialize(buffer: Uint8Array) -> Result<WasmTransaction, JsValue> {
    //     let tx = Transaction::deserialize_from_net(&buffer.to_vec());
    //     if tx.is_err() {
    //         return Err(JsValue::from("transaction deserialization failed"));
    //     }
    //     let tx = WasmTransaction::from_transaction(tx.unwrap());
    //     Ok(tx)
    // }
}

impl WasmTransaction {
    pub fn from_transaction(transaction: Transaction) -> WasmTransaction {
        WasmTransaction { tx: transaction }
    }
}

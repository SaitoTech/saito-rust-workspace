use std::io::{Error, ErrorKind};

use hex::encode;
use log::{debug, error, info, trace, warn};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::common::defs::{Currency, SaitoPublicKey, SaitoUTXOSetKey, UtxoSet};

/// The size of a serialized slip in bytes.
pub const SLIP_SIZE: usize = 59;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq, FromPrimitive)]
pub enum SlipType {
    Normal = 0,
    ATR = 1,
    VipInput = 2,
    VipOutput = 3,
    MinerInput = 4,
    MinerOutput = 5,
    RouterInput = 6,
    RouterOutput = 7,
    Other = 8,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Slip {
    #[serde_as(as = "[_; 33]")]
    pub public_key: SaitoPublicKey,
    pub amount: Currency,
    pub slip_index: u8,
    pub block_id: u64,
    pub tx_ordinal: u64,
    pub slip_type: SlipType,
    #[serde_as(as = "[_; 58]")]
    pub utxoset_key: SaitoUTXOSetKey,
    // TODO : Check if this can be removed with Option<>
    pub is_utxoset_key_set: bool,
}

impl Default for Slip {
    fn default() -> Self {
        Self {
            public_key: [0; 33],
            amount: 0,
            slip_index: 0,
            block_id: 0,
            tx_ordinal: 0,
            slip_type: SlipType::Normal,
            // uuid: [0; 32],
            utxoset_key: [0; 58],
            is_utxoset_key_set: false,
        }
    }
}

impl Slip {
    //
    // runs when block is purged for good or staking slip deleted
    //
    pub fn delete(&self, utxoset: &mut UtxoSet) -> bool {
        if self.get_utxoset_key() == [0; 58] {
            error!("ERROR 572034: asked to remove a slip without its utxoset_key properly set!");
            false;
        }
        let key_hex = encode(&self.get_utxoset_key());
        info!("remove {:?}", key_hex);
        utxoset.remove_entry(&self.get_utxoset_key());
        true
    }

    pub fn deserialize_from_net(bytes: &Vec<u8>) -> Result<Slip, Error> {
        if bytes.len() != SLIP_SIZE {
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        let public_key: SaitoPublicKey = bytes[..33].try_into().unwrap();
        let amount: Currency = Currency::from_be_bytes(bytes[33..41].try_into().unwrap());
        let block_id: u64 = u64::from_be_bytes(bytes[41..49].try_into().unwrap());
        let tx_ordinal: u64 = u64::from_be_bytes(bytes[49..57].try_into().unwrap());
        let slip_index: u8 = bytes[57];
        let slip_type: SlipType = FromPrimitive::from_u8(bytes[58]).unwrap();
        let mut slip = Slip::default();

        slip.public_key = public_key;
        slip.amount = amount;
        slip.block_id = block_id;
        slip.tx_ordinal = tx_ordinal;
        slip.slip_index = slip_index;
        slip.slip_type = slip_type;

        Ok(slip)
    }

    pub fn generate_utxoset_key(&mut self) {
        // if !self.is_utxoset_key_set {
        self.utxoset_key = self.get_utxoset_key();
        self.is_utxoset_key_set = true;
        // }
    }

    // 33 bytes public_key
    // 32 bytes uuid
    // 8 bytes amount
    // 1 byte slip_index
    pub fn get_utxoset_key(&self) -> SaitoUTXOSetKey {
        let res: Vec<u8> = vec![
            self.public_key.as_slice(),
            self.block_id.to_be_bytes().as_slice(),
            self.tx_ordinal.to_be_bytes().as_slice(),
            self.slip_index.to_be_bytes().as_slice(),
            self.amount.to_be_bytes().as_slice(),
        ]
        .concat();

        res[0..58].try_into().unwrap()
    }

    pub fn on_chain_reorganization(&self, utxoset: &mut UtxoSet, _lc: bool, spendable: bool) {
        if self.amount > 0 {
            debug!(
                "updating slip : {:?} as spendable : {:?}, block : {:?} tx : {:?} index : {:?}",
                hex::encode(self.utxoset_key),
                spendable,
                self.block_id,
                self.tx_ordinal,
                self.slip_index
            );
            let key_hex = encode(self.utxoset_key);
            info!("insert {:?}", key_hex);
            utxoset.insert(self.utxoset_key, spendable);
            // if utxoset.contains_key(&self.utxoset_key) {
            //     utxoset.insert(self.utxoset_key, spendable);
            // } else {
            //     utxoset.entry(self.utxoset_key).or_insert(spendable);
            // }
            // if spendable {
            //     utxoset.insert(self.utxoset_key, spendable);
            // } else {
            //     utxoset.remove(&self.utxoset_key);
            // }
        }
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let vbytes: Vec<u8> = [
            self.public_key.as_slice(),
            self.amount.to_be_bytes().as_slice(),
            self.block_id.to_be_bytes().as_slice(),
            self.tx_ordinal.to_be_bytes().as_slice(),
            self.slip_index.to_be_bytes().as_slice(),
            (self.slip_type as u8).to_be_bytes().as_slice(),
        ]
        .concat();
        assert_eq!(vbytes.len(), SLIP_SIZE);
        vbytes
    }

    pub fn serialize_input_for_signature(&self) -> Vec<u8> {
        [
            self.public_key.as_slice(),
            self.amount.to_be_bytes().as_slice(),
            // self.block_id.to_be_bytes().as_slice(),
            // self.tx_ordinal.to_be_bytes().as_slice(),
            self.slip_index.to_be_bytes().as_slice(),
            (self.slip_type as u8).to_be_bytes().as_slice(),
        ]
        .concat()
    }

    pub fn serialize_output_for_signature(&self) -> Vec<u8> {
        [
            self.public_key.as_slice(),
            self.amount.to_be_bytes().as_slice(),
            // self.block_id.to_be_bytes().as_slice(),
            // self.tx_ordinal.to_be_bytes().as_slice(),
            self.slip_index.to_be_bytes().as_slice(),
            (self.slip_type as u8).to_be_bytes().as_slice(),
        ]
        .concat()
    }

    pub fn validate(&self, utxoset: &UtxoSet) -> bool {
        if self.amount > 0 {
            match utxoset.get(&self.utxoset_key) {
                Some(value) => {
                    if *value {
                        true
                    } else {
                        // debug!() since method is used to check when cleaning up mempool
                        debug!(
                            "in utxoset but invalid: value is {} at {:?}, block : {:?} tx : {:?} index : {:?}",
                            *value,
                            hex::encode(self.utxoset_key),
                            self.block_id,
                            self.tx_ordinal,
                            self.slip_index
                        );
                        false
                    }
                }
                None => {
                    // debug!() since method is used to check when cleaning up mempool
                    debug!(
                        "not in utxoset so invalid. value is returned false: {:?} w/ type {:?} block : {:?} tx: {:?} index : {:?} and amount {:?}",
                        hex::encode(self.utxoset_key),
                        self.slip_type,
                        self.block_id,
                        self.tx_ordinal,
                        self.slip_index,
                        self.amount
                    );
                    false
                }
            }
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::common::defs::{push_lock, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_WALLET};
    use crate::core::data::blockchain::Blockchain;
    use crate::core::data::crypto::generate_keys;
    use crate::core::data::wallet::Wallet;
    use crate::{lock_for_read, lock_for_write};

    use super::*;

    #[test]
    fn slip_new_test() {
        let mut slip = Slip::default();
        assert_eq!(slip.public_key, [0; 33]);
        assert_eq!(slip.block_id, 0);
        assert_eq!(slip.tx_ordinal, 0);
        assert_eq!(slip.amount, 0);
        assert_eq!(slip.slip_type, SlipType::Normal);
        assert_eq!(slip.slip_index, 0);

        slip.public_key = [1; 33];
        assert_eq!(slip.public_key, [1; 33]);

        slip.amount = 100;
        assert_eq!(slip.amount, 100);

        slip.slip_index = 1;
        assert_eq!(slip.slip_index, 1);

        slip.slip_type = SlipType::MinerInput;
        assert_eq!(slip.slip_type, SlipType::MinerInput);
    }

    #[test]
    fn slip_serialize_for_signature_test() {
        let slip = Slip::default();
        assert_eq!(
            slip.serialize_input_for_signature(),
            vec![0; SLIP_SIZE - 16]
        );
        assert_eq!(
            slip.serialize_output_for_signature(),
            vec![0; SLIP_SIZE - 16]
        );
        assert_eq!(slip.serialize_for_net(), vec![0; SLIP_SIZE]);
    }

    #[test]
    fn slip_get_utxoset_key_test() {
        let slip = Slip::default();
        assert_eq!(slip.get_utxoset_key(), [0; 58]);
    }

    #[test]
    fn slip_serialization_for_net_test() {
        let slip = Slip::default();
        let serialized_slip = slip.serialize_for_net();
        assert_eq!(serialized_slip.len(), SLIP_SIZE);
        let deserilialized_slip = Slip::deserialize_from_net(&serialized_slip).unwrap();
        assert_eq!(slip, deserilialized_slip);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn slip_addition_and_removal_from_utxoset() {
        let keys = generate_keys();
        let wallet_lock = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (mut blockchain, _blockchain_) =
            lock_for_write!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        let mut slip = Slip::default();
        slip.amount = 100_000;
        slip.block_id = 10;
        slip.tx_ordinal = 20;
        {
            let (wallet, _wallet_) = lock_for_read!(wallet_lock, LOCK_ORDER_WALLET);
            slip.public_key = wallet.public_key;
        }
        slip.generate_utxoset_key();

        // add to utxoset
        slip.on_chain_reorganization(&mut blockchain.utxoset, true, true);
        assert!(blockchain.utxoset.contains_key(&slip.get_utxoset_key()));

        // remove from utxoset
        // TODO: Repair this test
        // slip.purge(&mut blockchain.utxoset);
        // assert_eq!(
        //     blockchain.utxoset.contains_key(&slip.get_utxoset_key()),
        //     false
        // );
    }
}

use log::{error, warn};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoUTXOSetKey, UtxoSet};

/// The size of a serilized slip in bytes.
pub const SLIP_SIZE: usize = 75;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq, FromPrimitive)]
pub enum SlipType {
    Normal,
    ATR,
    VipInput,
    VipOutput,
    MinerInput,
    MinerOutput,
    RouterInput,
    RouterOutput,
    Other,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Slip {
    #[serde_as(as = "[_; 33]")]
    pub public_key: SaitoPublicKey,
    pub amount: u64,
    pub slip_index: u8,
    pub slip_type: SlipType,
    pub uuid: SaitoHash,
    #[serde_as(as = "[_; 74]")]
    pub utxoset_key: SaitoUTXOSetKey,
    pub is_utxoset_key_set: bool,
}

impl Slip {
    pub fn new() -> Self {
        Self {
            public_key: [0; 33],
            amount: 0,
            slip_index: 0,
            slip_type: SlipType::Normal,
            uuid: [0; 32],
            utxoset_key: [0; 74],
            is_utxoset_key_set: false,
        }
    }

    //
    // runs when block is purged for good or staking slip deleted
    //
    pub fn delete(&self, utxoset: &mut UtxoSet) -> bool {
        if self.get_utxoset_key() == [0; 74] {
            error!("ERROR 572034: asked to remove a slip without its utxoset_key properly set!");
            false;
        }
        utxoset.remove_entry(&self.get_utxoset_key());
        true
    }

    pub fn deserialize_from_net(bytes: Vec<u8>) -> Slip {
        let public_key: SaitoPublicKey = bytes[..33].try_into().unwrap();
        let uuid: SaitoHash = bytes[33..65].try_into().unwrap();
        let amount: u64 = u64::from_be_bytes(bytes[65..73].try_into().unwrap());
        let slip_index: u8 = bytes[73];
        let slip_type: SlipType = FromPrimitive::from_u8(bytes[SLIP_SIZE - 1]).unwrap();
        let mut slip = Slip::new();

        slip.public_key = public_key;
        slip.uuid = uuid;
        slip.amount = amount;
        slip.slip_index = slip_index;
        slip.slip_type = slip_type;

        slip
    }

    pub fn generate_utxoset_key(&mut self) {
        self.utxoset_key = self.get_utxoset_key();
        self.is_utxoset_key_set = true;
    }

    // 33 bytes public_key
    // 32 bytes uuid
    // 8 bytes amount
    // 1 byte slip_index
    pub fn get_utxoset_key(&self) -> SaitoUTXOSetKey {
        let mut res: Vec<u8> = vec![];
        res.extend(&self.public_key);
        res.extend(&self.uuid);
        res.extend(&self.amount.to_be_bytes());
        res.extend(&self.slip_index.to_be_bytes());

        res[0..74].try_into().unwrap()
    }

    pub fn on_chain_reorganization(&self, utxoset: &mut UtxoSet, _lc: bool, spendable: bool) {
        if self.amount > 0 {
            if utxoset.contains_key(&self.utxoset_key) {
                utxoset.insert(self.utxoset_key, spendable);
            } else {
                utxoset.entry(self.utxoset_key).or_insert(spendable);
            }
        }
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.public_key);
        vbytes.extend(&self.uuid);
        vbytes.extend(&self.amount.to_be_bytes());
        vbytes.extend(&self.slip_index.to_be_bytes());
        vbytes.extend(&(self.slip_type as u8).to_be_bytes());
        assert_eq!(vbytes.len(), SLIP_SIZE);
        vbytes
    }

    pub fn serialize_input_for_signature(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.public_key);
        vbytes.extend(&self.uuid);
        vbytes.extend(&self.amount.to_be_bytes());
        vbytes.extend(&(self.slip_index.to_be_bytes()));
        vbytes.extend(&(self.slip_type as u8).to_be_bytes());
        vbytes
    }

    pub fn serialize_output_for_signature(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.public_key);
        vbytes.extend(&[0; 32]);
        vbytes.extend(&self.amount.to_be_bytes());
        vbytes.extend(&(self.slip_index.to_be_bytes()));
        vbytes.extend(&(self.slip_type as u8).to_be_bytes());
        vbytes
    }

    pub fn validate(&self, utxoset: &UtxoSet) -> bool {
        if self.amount > 0 {
            match utxoset.get(&self.utxoset_key) {
                Some(value) => {
                    if *value == true {
                        true
                    } else {
                        warn!(
                            "in utxoset but invalid: value is {} at {:?}",
                            *value,
                            hex::encode(self.utxoset_key)
                        );
                        false
                    }
                }
                None => {
                    warn!("not in utxoset so invalid");
                    error!(
                        "value is returned false: {:?} w/ type {:?}  ordinal {} and amount {}",
                        hex::encode(self.utxoset_key),
                        self.slip_type,
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

    use log::trace;
    use tokio::sync::RwLock;

    use crate::core::data::blockchain::Blockchain;
    use crate::core::data::wallet::Wallet;

    use super::*;

    #[test]
    fn slip_new_test() {
        let mut slip = Slip::new();
        assert_eq!(slip.public_key, [0; 33]);
        assert_eq!(slip.uuid, [0; 32]);
        assert_eq!(slip.amount, 0);
        assert_eq!(slip.slip_type, SlipType::Normal);
        assert_eq!(slip.slip_index, 0);

        slip.public_key = [1; 33];
        assert_eq!(slip.public_key, [1; 33]);

        slip.amount = 100;
        assert_eq!(slip.amount, 100);

        slip.uuid = [30; 32];
        assert_eq!(slip.uuid, [30; 32]);

        slip.slip_index = 1;
        assert_eq!(slip.slip_index, 1);

        slip.slip_type = SlipType::MinerInput;
        assert_eq!(slip.slip_type, SlipType::MinerInput);
    }

    #[test]
    fn slip_serialize_for_signature_test() {
        let slip = Slip::new();
        assert_eq!(slip.serialize_input_for_signature(), vec![0; SLIP_SIZE]);
    }

    #[test]
    fn slip_get_utxoset_key_test() {
        let slip = Slip::new();
        assert_eq!(slip.get_utxoset_key(), [0; 74]);
    }

    #[test]
    fn slip_serialization_for_net_test() {
        let slip = Slip::new();
        let serialized_slip = slip.serialize_for_net();
        assert_eq!(serialized_slip.len(), 75);
        let deserilialized_slip = Slip::deserialize_from_net(serialized_slip);
        assert_eq!(slip, deserilialized_slip);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn slip_addition_and_removal_from_utxoset() {
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        trace!("waiting for the blockchain write lock");
        let mut blockchain = blockchain_lock.write().await;
        trace!("acquired the blockchain write lock");

        let mut slip = Slip::new();
        slip.amount = 100_000;
        slip.uuid = [1; 32];
        {
            trace!("waiting for the wallet write lock");
            let wallet = wallet_lock.read().await;
            trace!("acquired the wallet write lock");
            slip.public_key = wallet.public_key;
        }
        slip.generate_utxoset_key();

        // add to utxoset
        slip.on_chain_reorganization(&mut blockchain.utxoset, true, true);
        assert_eq!(
            blockchain.utxoset.contains_key(&slip.get_utxoset_key()),
            true
        );

        // remove from utxoset
        // TODO: Repair this test
        // slip.purge(&mut blockchain.utxoset);
        // assert_eq!(
        //     blockchain.utxoset.contains_key(&slip.get_utxoset_key()),
        //     false
        // );
    }
}

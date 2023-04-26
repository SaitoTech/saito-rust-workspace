use std::io::{Error, ErrorKind};

use serde::{Deserialize, Serialize};

use crate::common::defs::{SaitoPrivateKey, SaitoPublicKey, SaitoSignature};
use crate::core::data::crypto::sign;
use crate::core::data::transaction::Transaction;

pub const HOP_SIZE: usize = 130;

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Hop {
    #[serde_as(as = "[_; 33]")]
    pub(crate) from: SaitoPublicKey,
    #[serde_as(as = "[_; 33]")]
    pub(crate) to: SaitoPublicKey,
    #[serde_as(as = "[_; 64]")]
    pub(crate) sig: SaitoSignature,
}

impl Default for Hop {
    fn default() -> Self {
        Hop {
            from: [0; 33],
            to: [0; 33],
            sig: [0; 64],
        }
    }
}

impl Hop {
    pub fn generate(
        my_private_key: &SaitoPrivateKey,
        my_public_key: &SaitoPublicKey,
        to_public_key: &SaitoPublicKey,
        tx: &Transaction,
    ) -> Hop {
        let mut hop = Hop::default();

        // msg-to-sign is hash of transaction signature + next_peer.public_key
        let buffer: Vec<u8> = [tx.signature.as_slice(), to_public_key.as_slice()].concat();

        hop.from = my_public_key.clone();
        hop.to = to_public_key.clone();
        hop.sig = sign(buffer.as_slice(), &my_private_key);

        hop
    }

    pub fn deserialize_from_net(bytes: &Vec<u8>) -> Result<Hop, Error> {
        if bytes.len() != HOP_SIZE {
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        let from: SaitoPublicKey = bytes[..33].try_into().unwrap();
        let to: SaitoPublicKey = bytes[33..66].try_into().unwrap();
        let sig: SaitoSignature = bytes[66..130].try_into().unwrap();

        let mut hop = Hop::default();
        hop.from = from;
        hop.to = to;
        hop.sig = sig;

        Ok(hop)
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let vbytes: Vec<u8> = [
            self.from.as_slice(),
            self.to.as_slice(),
            self.sig.as_slice(),
        ]
        .concat();
        // vbytes.extend(&self.from);
        // vbytes.extend(&self.to);
        // vbytes.extend(&self.sig);
        vbytes
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::common::defs::{push_lock, LOCK_ORDER_WALLET};
    use crate::core::data::crypto::{generate_keys, verify};
    use crate::core::data::hop::Hop;
    use crate::core::data::wallet::Wallet;
    use crate::lock_for_read;

    use super::*;

    #[test]
    fn hop_new_test() {
        let hop = Hop::default();
        assert_eq!(hop.from, [0; 33]);
        assert_eq!(hop.to, [0; 33]);
        assert_eq!(hop.sig, [0; 64]);
    }

    #[tokio::test]
    async fn generate_test() {
        let keys = generate_keys();
        let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
        let sender_public_key: SaitoPublicKey;

        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);

            sender_public_key = wallet.public_key;
        }

        let tx = Transaction::default();
        let (receiver_public_key, _receiver_private_key) = generate_keys();

        let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
        let hop = Hop::generate(
            &wallet.private_key,
            &wallet.public_key,
            &receiver_public_key,
            &tx,
        );

        assert_eq!(hop.from, sender_public_key);
        assert_eq!(hop.to, receiver_public_key);
    }

    #[tokio::test]
    async fn serialize_and_deserialize_test() {
        let keys = generate_keys();
        let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
        let mut tx = Transaction::default();
        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
            tx.sign(&wallet.private_key);
        }
        let (receiver_public_key, _receiver_private_key) = generate_keys();

        let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
        let hop = Hop::generate(
            &wallet.private_key,
            &wallet.public_key,
            &receiver_public_key,
            &tx,
        );

        let hop2 = Hop::deserialize_from_net(&hop.serialize_for_net()).unwrap();

        assert_eq!(hop.from, hop2.from);
        assert_eq!(hop.to, hop2.to);
        assert_eq!(hop.sig, hop2.sig);

        let mut buffer = vec![];
        buffer.extend(tx.signature.to_vec());
        buffer.extend(hop.to.to_vec());
        let result = verify(buffer.as_slice(), &hop.sig, &hop.from);
        assert!(result);
    }
}

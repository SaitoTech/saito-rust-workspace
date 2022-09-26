use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoPublicKey, SaitoSignature};
use crate::core::data::crypto::{hash, sign};
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::{log_read_lock_receive, log_read_lock_request};

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

impl Hop {
    pub fn new() -> Self {
        Hop {
            from: [0; 33],
            to: [0; 33],
            sig: [0; 64],
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn generate(wallet: &Wallet, to_public_key: SaitoPublicKey, tx: &Transaction) -> Hop {
        let mut hop = Hop::new();

        //
        // msg-to-sign is hash of transaction signature + next_peer.public_key
        //
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(tx.signature);
        vbytes.extend(&to_public_key);
        let hash_to_sign = hash(&vbytes);

        hop.from = wallet.public_key;
        hop.to = to_public_key;
        hop.sig = sign(&hash_to_sign, wallet.private_key);

        hop
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn deserialize_from_net(bytes: &Vec<u8>) -> Hop {
        let from: SaitoPublicKey = bytes[..33].try_into().unwrap();
        let to: SaitoPublicKey = bytes[33..66].try_into().unwrap();
        let sig: SaitoSignature = bytes[66..130].try_into().unwrap();

        let mut hop = Hop::new();
        hop.from = from;
        hop.to = to;
        hop.sig = sig;

        hop
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn serialize_for_net(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.from);
        vbytes.extend(&self.to);
        vbytes.extend(&self.sig);
        vbytes
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::crypto::{generate_keys, verify};
    use crate::core::data::hop::Hop;

    use super::*;

    #[test]
    fn hop_new_test() {
        let hop = Hop::new();
        assert_eq!(hop.from, [0; 33]);
        assert_eq!(hop.to, [0; 33]);
        assert_eq!(hop.sig, [0; 64]);
    }

    #[tokio::test]
    async fn generate_test() {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let sender_public_key: SaitoPublicKey;

        {
            let w = wallet.read().await;
            sender_public_key = w.public_key;
        }

        let tx = Transaction::new();
        let (receiver_public_key, _receiver_private_key) = generate_keys();

        let wallet = wallet.read().await;
        let hop = Hop::generate(&wallet, receiver_public_key, &tx).await;

        assert_eq!(hop.from, sender_public_key);
        assert_eq!(hop.to, receiver_public_key);
    }

    #[tokio::test]
    async fn serialize_and_deserialize_test() {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let mut tx = Transaction::new();
        {
            let w = wallet.read().await;
            tx.sign(w.private_key);
        }
        let (receiver_public_key, _receiver_private_key) = generate_keys();

        let wallet = wallet.read().await;
        let hop = Hop::generate(&wallet, receiver_public_key, &tx).await;

        let hop2 = Hop::deserialize_from_net(&hop.serialize_for_net());

        assert_eq!(hop.from, hop2.from);
        assert_eq!(hop.to, hop2.to);
        assert_eq!(hop.sig, hop2.sig);

        let mut buffer = vec![];
        buffer.extend(tx.signature.to_vec());
        buffer.extend(hop.to.to_vec());
        let hash = hash(&buffer);
        let result = verify(&hash, hop.sig, hop.from);
        assert!(result);
    }
}

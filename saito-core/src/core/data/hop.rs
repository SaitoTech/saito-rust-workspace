use std::sync::Arc;

use log::trace;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoPublicKey, SaitoSignature};
use crate::core::data::crypto::{hash, sign};
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

pub const HOP_SIZE: usize = 130;

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Hop {
    #[serde_as(as = "[_; 33]")]
    from: SaitoPublicKey,
    #[serde_as(as = "[_; 33]")]
    to: SaitoPublicKey,
    #[serde_as(as = "[_; 64]")]
    sig: SaitoSignature,
}

impl Hop {
    pub fn new() -> Self {
        Hop {
            from: [0; 33],
            to: [0; 33],
            sig: [0; 64],
        }
    }

    pub async fn generate(
        wallet_lock: Arc<RwLock<Wallet>>,
        to_public_key: SaitoPublicKey,
        tx: &Transaction,
    ) -> Hop {
        trace!("waiting for the wallet read lock");
        let wallet = wallet_lock.read().await;
        trace!("acquired the wallet read lock");
        let mut hop = Hop::new();

        //
        // msg-to-sign is hash of transaction signature + next_peer.public_key
        //
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(tx.get_signature());
        vbytes.extend(&to_public_key);
        let hash_to_sign = hash(&vbytes);

        hop.set_from(wallet.public_key);
        hop.set_to(to_public_key);
        hop.set_sig(sign(&hash_to_sign, wallet.private_key));

        hop
    }

    pub fn get_from(&self) -> SaitoPublicKey {
        self.from
    }

    pub fn get_to(&self) -> SaitoPublicKey {
        self.to
    }

    pub fn get_sig(&self) -> SaitoSignature {
        self.sig
    }

    pub fn set_from(&mut self, from: SaitoPublicKey) {
        self.from = from
    }

    pub fn set_to(&mut self, to: SaitoPublicKey) {
        self.to = to
    }

    pub fn set_sig(&mut self, sig: SaitoSignature) {
        self.sig = sig
    }

    pub fn deserialize_from_net(bytes: Vec<u8>) -> Hop {
        let from: SaitoPublicKey = bytes[..33].try_into().unwrap();
        let to: SaitoPublicKey = bytes[33..66].try_into().unwrap();
        let sig: SaitoSignature = bytes[66..130].try_into().unwrap();

        let mut hop = Hop::new();
        hop.set_from(from);
        hop.set_to(to);
        hop.set_sig(sig);

        hop
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.get_from());
        vbytes.extend(&self.get_to());
        vbytes.extend(&self.get_sig());
        vbytes
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::core::data::crypto::generate_keys;
    use crate::core::data::hop::Hop;

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

        let hop = Hop::generate(wallet, receiver_public_key, &tx).await;

        assert_eq!(hop.from, sender_public_key);
        assert_eq!(hop.to, receiver_public_key);
    }

    #[tokio::test]
    async fn serialize_and_deserialize_test() {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let tx = Transaction::new();
        let (receiver_public_key, _receiver_private_key) = generate_keys();

        let hop = Hop::generate(wallet, receiver_public_key, &tx).await;

        let hop2 = Hop::deserialize_from_net(hop.serialize_for_net());

        assert_eq!(hop.from, hop2.from);
        assert_eq!(hop.to, hop2.to);
        assert_eq!(hop.sig, hop2.sig);
    }
}

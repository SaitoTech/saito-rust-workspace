use std::sync::Arc;

use log::trace;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoSignature};
use crate::core::data::crypto::sign;
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

    pub async fn generate_hop(
        wallet_lock: Arc<RwLock<Wallet>>,
        to_publickey: SaitoPublicKey,
        hash_to_sign: SaitoHash,
    ) -> Hop {
        trace!("waiting for the wallet read lock");
        let wallet = wallet_lock.read().await;
        trace!("acquired the wallet read lock");
        let mut hop = Hop::new();

        hop.set_from(wallet.get_publickey());
        hop.set_to(to_publickey);
        hop.set_sig(sign(&hash_to_sign, wallet.get_privatekey()));

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

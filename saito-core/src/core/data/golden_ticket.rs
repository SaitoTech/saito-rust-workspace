use std::convert::TryInto;

use bigint::uint::U256;
use log::trace;
use serde::{Deserialize, Serialize};

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::core::data::crypto::hash;

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GoldenTicket {
    target: SaitoHash,
    random: SaitoHash,
    #[serde_as(as = "[_; 33]")]
    publickey: SaitoPublicKey,
}

impl GoldenTicket {
    #[allow(clippy::new_without_default)]

    pub fn new(target: SaitoHash, random: SaitoHash, publickey: SaitoPublicKey) -> Self {
        return Self {
            target,
            random,
            publickey,
        };
    }

    pub fn create(
        previous_block_hash: SaitoHash,
        random_bytes: SaitoHash,
        publickey: SaitoPublicKey,
    ) -> GoldenTicket {
        GoldenTicket::new(previous_block_hash, random_bytes, publickey)
    }

    pub fn deserialize_from_net(bytes: Vec<u8>) -> GoldenTicket {
        let target: SaitoHash = bytes[0..32].try_into().unwrap();
        let random: SaitoHash = bytes[32..64].try_into().unwrap();
        let publickey: SaitoPublicKey = bytes[64..97].try_into().unwrap();
        GoldenTicket::new(target, random, publickey)
    }

    pub fn get_target(&self) -> SaitoHash {
        self.target
    }

    pub fn get_random(&self) -> SaitoHash {
        self.random
    }

    pub fn get_publickey(&self) -> SaitoPublicKey {
        self.publickey
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.target);
        vbytes.extend(&self.random);
        vbytes.extend(&self.publickey);
        vbytes
    }

    pub fn validate(&self, difficulty: u64) -> bool {
        let solution_hash = hash(&self.serialize_for_net());
        return GoldenTicket::validate_hashing_difficulty(&solution_hash, difficulty);
    }

    pub fn validate_hashing_difficulty(solution_hash: &SaitoHash, difficulty: u64) -> bool {
        let solution = U256::from_big_endian(solution_hash);

        if solution.leading_zeros() >= difficulty as u32 {
            trace!(
                "GT : difficulty : {:?} solution : {:?} ",
                difficulty,
                hex::encode(solution_hash)
            );

            return true;
        }

        return false;
    }
}

#[cfg(test)]
mod tests {
    use crate::common::defs::SaitoHash;
    use crate::core::data::crypto::{generate_random_bytes, hash};
    use crate::core::data::golden_ticket::GoldenTicket;
    use crate::core::data::wallet::Wallet;

    #[test]
    fn golden_ticket_validate_hashing_difficulty() {
        let hash: SaitoHash = [0u8; 32];
        let mut hash2: SaitoHash = [255u8; 32];

        assert!(GoldenTicket::validate_hashing_difficulty(&hash, 0));
        assert!(GoldenTicket::validate_hashing_difficulty(&hash, 10));
        assert!(GoldenTicket::validate_hashing_difficulty(&hash, 256));
        assert_eq!(
            GoldenTicket::validate_hashing_difficulty(&hash, 1000000),
            false
        );

        assert!(GoldenTicket::validate_hashing_difficulty(&hash2, 0));
        assert_eq!(GoldenTicket::validate_hashing_difficulty(&hash2, 10), false);
        assert_eq!(
            GoldenTicket::validate_hashing_difficulty(&hash2, 256),
            false
        );

        hash2[0] = 15u8;

        assert!(GoldenTicket::validate_hashing_difficulty(&hash2, 3));
        assert!(GoldenTicket::validate_hashing_difficulty(&hash2, 4));
        assert_eq!(GoldenTicket::validate_hashing_difficulty(&hash2, 5), false);
    }

    #[test]
    fn golden_ticket_extremes_test() {
        let wallet = Wallet::new();

        let random = hash(&generate_random_bytes(32));
        let target = hash(&random.to_vec());
        let publickey = wallet.get_publickey();

        let gt = GoldenTicket::create(target, random, publickey);

        assert_eq!(gt.validate(0), true);
        assert_eq!(gt.validate(256), false);
    }
}

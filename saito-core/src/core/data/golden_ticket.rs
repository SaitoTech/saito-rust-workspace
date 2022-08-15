use std::convert::TryInto;
use std::ops::{Shl, Shr};

use log::{debug, trace};

use secp256k1::bitcoin_hashes::hex::ToHex;
use serde::{Deserialize, Serialize};

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::core::data::crypto::hash;

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GoldenTicket {
    pub target: SaitoHash,
    pub(crate) random: SaitoHash,
    #[serde_as(as = "[_; 33]")]
    pub(crate) public_key: SaitoPublicKey,
}

impl GoldenTicket {
    #[allow(clippy::new_without_default)]

    pub fn new(target: SaitoHash, random: SaitoHash, public_key: SaitoPublicKey) -> Self {
        return Self {
            target,
            random,
            public_key,
        };
    }

    pub fn create(
        previous_block_hash: SaitoHash,
        random_bytes: SaitoHash,
        public_key: SaitoPublicKey,
    ) -> GoldenTicket {
        GoldenTicket::new(previous_block_hash, random_bytes, public_key)
    }

    pub fn deserialize_from_net(bytes: Vec<u8>) -> GoldenTicket {
        assert_eq!(bytes.len(), 97);
        let target: SaitoHash = bytes[0..32].try_into().unwrap();
        let random: SaitoHash = bytes[32..64].try_into().unwrap();
        let public_key: SaitoPublicKey = bytes[64..97].try_into().unwrap();
        GoldenTicket::new(target, random, public_key)
    }

    pub fn serialize_for_net(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.target);
        vbytes.extend(&self.random);
        vbytes.extend(&self.public_key);
        vbytes
    }

    pub fn validate(&self, difficulty: u64) -> bool {
        let solution_hash = hash(&self.serialize_for_net());

        trace!("gt sol hash = {:?}", hex::encode(solution_hash));

        return GoldenTicket::validate_hashing_difficulty(&solution_hash, difficulty);
    }

    pub fn validate_hashing_difficulty(solution_hash: &SaitoHash, difficulty: u64) -> bool {
        let solution = primitive_types::U256::from_big_endian(solution_hash);

        if solution.leading_zeros() >= difficulty as u32 {
            debug!(
                "GT : difficulty : {:?} solution : {:?}",
                difficulty,
                hex::encode(solution_hash)
            );

            return true;
        }
        trace!(
            "difficulty = {:?} leading zeros = {:?}",
            difficulty,
            solution.leading_zeros()
        );
        return false;
    }
}

#[cfg(test)]
mod tests {
    use crate::common::defs::SaitoHash;
    use crate::core::data::crypto::{generate_random_bytes, hash};
    use crate::core::data::golden_ticket::GoldenTicket;
    use crate::core::data::wallet::Wallet;
    use log::{debug, info};

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
        let public_key = wallet.public_key;

        let gt = GoldenTicket::create(target, random, public_key);

        assert_eq!(gt.validate(0), true);
        assert_eq!(gt.validate(256), false);
    }
    #[test]
    fn gt_against_slr() {
        let buffer = hex::decode("844702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235e03bf1a4714cfc7ae33d3f6e860c23191ddea07bcb1bfa6c85bc124151ad8d4ce03cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610").unwrap();
        assert_eq!(buffer.len(), 97);

        let result = GoldenTicket::deserialize_from_net(buffer);
        assert_eq!(
            hex::encode(result.target),
            "844702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235e"
        );
        assert_eq!(
            hex::encode(result.random),
            "03bf1a4714cfc7ae33d3f6e860c23191ddea07bcb1bfa6c85bc124151ad8d4ce"
        );
        assert_eq!(
            hex::encode(result.public_key),
            "03cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610"
        );

        assert!(result.validate(0));
    }

    #[test]
    fn gt_against_slr_2() {
        pretty_env_logger::init();

        assert_eq!(primitive_types::U256::one().leading_zeros(), 255);
        assert_eq!(primitive_types::U256::zero().leading_zeros(), 256);
        let sol = hex::decode("4523d0eb05233434b42de74a99049decb6c4347da2e7cde9fb49330e905da1e2")
            .unwrap();
        info!("sss = {:?}", sol);
        assert_eq!(
            primitive_types::U256::from_big_endian(sol.as_ref()).leading_zeros(),
            1
        );

        let gt = GoldenTicket {
            target: hex::decode("6bc717fdd325b39383923e21c00aedf04efbc2d8ae6ba092e86b984ba45daf5f")
                .unwrap()
                .to_vec()
                .try_into()
                .unwrap(),
            random: hex::decode("e41eed52c0d1b261654bd7bc7c15996276714e79bf837e129b022f9c04a97e49")
                .unwrap()
                .to_vec()
                .try_into()
                .unwrap(),
            public_key: hex::decode(
                "02262b7491f6599ed3f4f60315d9345e9ef02767973663b9764b52842306da461c",
            )
            .unwrap()
            .to_vec()
            .try_into()
            .unwrap(),
        };

        let result = gt.validate(1);

        assert!(result);
    }
}

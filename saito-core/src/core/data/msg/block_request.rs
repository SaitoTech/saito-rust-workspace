use std::io::{Error, ErrorKind};

use crate::common::defs::SaitoHash;
use crate::core::data::serialize::Serialize;

#[derive(Debug)]
pub struct BlockchainRequest {
    latest_block_id: u64,
    latest_block_hash: SaitoHash,
    fork_id: Vec<u8>,
}

impl Serialize<Self> for BlockchainRequest {
    fn serialize(&self) -> Vec<u8> {
        [
            self.latest_block_id.to_be_bytes().to_vec(),
            self.latest_block_hash.to_vec(),
            self.fork_id.clone(),
        ]
        .concat()
    }

    fn deserialize(buffer: &Vec<u8>) -> Result<Self, Error> {
        if buffer.len() < 40 {
            return Err(Error::from(ErrorKind::InvalidData));
        }
        Ok(BlockchainRequest {
            latest_block_id: u64::from_be_bytes(buffer[0..8].to_vec().try_into().unwrap()),
            latest_block_hash: buffer[8..40].to_vec().try_into().unwrap(),
            fork_id: buffer[40..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::crypto::generate_random_bytes;
    use crate::core::data::msg::block_request::BlockchainRequest;
    use crate::core::data::serialize::Serialize;

    #[test]
    fn test_serialize_with_full_fork_id() {
        let request = BlockchainRequest {
            latest_block_id: 10,
            latest_block_hash: generate_random_bytes(32).try_into().unwrap(),
            fork_id: generate_random_bytes(32),
        };
        let buffer = request.serialize();
        assert_eq!(buffer.len(), 72);
        let new_request = BlockchainRequest::deserialize(&buffer);
        assert!(new_request.is_ok());
        let new_request = new_request.unwrap();
        assert_eq!(request.latest_block_id, new_request.latest_block_id);
        assert_eq!(request.latest_block_hash, new_request.latest_block_hash);
        assert_eq!(request.fork_id, new_request.fork_id);
    }

    #[test]
    fn test_serialize_with_short_fork_id() {
        let request = BlockchainRequest {
            latest_block_id: 10,
            latest_block_hash: generate_random_bytes(32).try_into().unwrap(),
            fork_id: generate_random_bytes(10),
        };
        let buffer = request.serialize();
        assert_eq!(buffer.len(), 50);
        let new_request = BlockchainRequest::deserialize(&buffer);
        assert!(new_request.is_ok());
        let new_request = new_request.unwrap();
        assert_eq!(request.latest_block_id, new_request.latest_block_id);
        assert_eq!(request.latest_block_hash, new_request.latest_block_hash);
        assert_eq!(request.fork_id, new_request.fork_id);
    }
    #[test]
    fn test_serialize_without_fork_id() {
        let request = BlockchainRequest {
            latest_block_id: 10,
            latest_block_hash: generate_random_bytes(32).try_into().unwrap(),
            fork_id: vec![],
        };
        let buffer = request.serialize();
        assert_eq!(buffer.len(), 40);
        let new_request = BlockchainRequest::deserialize(&buffer);
        assert!(new_request.is_ok());
        let new_request = new_request.unwrap();
        assert_eq!(request.latest_block_id, new_request.latest_block_id);
        assert_eq!(request.latest_block_hash, new_request.latest_block_hash);
        assert_eq!(request.fork_id, new_request.fork_id);
        assert!(new_request.fork_id.is_empty());
    }
}

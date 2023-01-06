use std::io::{Error, ErrorKind};

use tracing::{trace, warn};

use crate::common::defs::SaitoHash;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::data::serialize::Serialize;
use crate::core::data::transaction::Transaction;

#[derive(Debug)]
pub enum Message {
    HandshakeChallenge(HandshakeChallenge),
    HandshakeResponse(HandshakeResponse),
    ApplicationMessage(Vec<u8>),
    Block(Block),
    Transaction(Transaction),
    BlockchainRequest(BlockchainRequest),
    BlockHeaderHash(SaitoHash, u64),
    Ping(),
    SPVChain(),
    Services(),
    GhostChain(),
    GhostChainRequest(),
    Result(),
    Error(),
    ApplicationTransaction(Vec<u8>),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let message_type: u8 = self.get_type_value();
        let request_id: u32 = 0;
        let mut buffer: Vec<u8> = vec![];
        buffer.extend(&message_type.to_be_bytes());
        buffer.extend(&request_id.to_be_bytes());
        buffer.append(&mut match self {
            Message::HandshakeChallenge(data) => data.serialize(),
            Message::HandshakeResponse(data) => data.serialize(),
            Message::ApplicationMessage(data) => data.clone(),
            Message::ApplicationTransaction(data) => data.clone(),
            Message::Block(data) => data.serialize_for_net(BlockType::Full),
            Message::Transaction(data) => data.serialize_for_net(),
            Message::BlockchainRequest(data) => data.serialize(),
            Message::BlockHeaderHash(block_hash, block_id) => {
                [block_hash.as_slice(), block_id.to_be_bytes().as_slice()].concat()
            }
            Message::Ping() => {
                vec![]
            }
            _ => {
                todo!()
            }
        });

        return buffer;
    }
    pub fn deserialize(buffer: Vec<u8>) -> Result<Message, Error> {
        let message_type: u8 = u8::from_be_bytes(buffer[0..1].try_into().unwrap());
        let _request_id: u32 = u32::from_be_bytes(buffer[1..5].try_into().unwrap());
        let buffer = buffer[5..].to_vec();

        trace!("buffer size = {:?}", buffer.len());

        // TODO : remove hardcoded values into an enum
        match message_type {
            1 => {
                let result = HandshakeChallenge::deserialize(&buffer)?;
                Ok(Message::HandshakeChallenge(result))
            }
            2 => {
                let result = HandshakeResponse::deserialize(&buffer)?;
                Ok(Message::HandshakeResponse(result))
            }

            4 => Ok(Message::ApplicationMessage(buffer)),
            5 => {
                let block = Block::deserialize_from_net(buffer)?;
                Ok(Message::Block(block))
            }
            6 => {
                let tx = Transaction::deserialize_from_net(&buffer);
                Ok(Message::Transaction(tx))
            }
            7 => {
                let result = BlockchainRequest::deserialize(&buffer)?;
                Ok(Message::BlockchainRequest(result))
            }
            8 => {
                assert_eq!(buffer.len(), 40);
                let block_hash = buffer[0..32].to_vec().try_into().unwrap();
                let block_id = u64::from_be_bytes(buffer[32..40].to_vec().try_into().unwrap());
                Ok(Message::BlockHeaderHash(block_hash, block_id))
            }
            9 => Ok(Message::Ping()),
            10 => Ok(Message::SPVChain()),
            11 => Ok(Message::Services()),
            12 => Ok(Message::GhostChain()),
            13 => Ok(Message::GhostChainRequest()),
            14 => Ok(Message::Result()),
            15 => Ok(Message::Error()),
            16 => Ok(Message::ApplicationTransaction(buffer)),
            _ => {
                warn!("message type : {:?} not valid", message_type);
                Err(Error::from(ErrorKind::InvalidData))
            }
        }
    }
    pub fn get_type_value(&self) -> u8 {
        match self {
            Message::HandshakeChallenge(_) => 1,
            Message::HandshakeResponse(_) => 2,
            Message::ApplicationMessage(_) => 4,
            Message::Block(_) => 5,
            Message::Transaction(_) => 6,
            Message::BlockchainRequest(_) => 7,
            Message::BlockHeaderHash(_, _) => 8,
            Message::Ping() => 9,
            Message::SPVChain() => 10,
            Message::Services() => 11,
            Message::GhostChain() => 12,
            Message::GhostChainRequest() => 13,
            Message::Result() => 14,
            Message::Error() => 15,
            Message::ApplicationTransaction(_) => 16,
        }
    }
}

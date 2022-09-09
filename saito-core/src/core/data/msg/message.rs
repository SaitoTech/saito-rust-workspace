use std::io::{Error, ErrorKind};

use tracing::{trace, warn};

use crate::common::defs::SaitoHash;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::serialize::Serialize;
use crate::core::data::transaction::Transaction;

#[derive(Debug)]
pub enum Message {
    HandshakeChallenge(HandshakeChallenge),
    HandshakeResponse(HandshakeResponse),
    HandshakeCompletion(HandshakeCompletion),
    ApplicationMessage(Vec<u8>),
    Block(Block),
    Transaction(Transaction),
    BlockchainRequest(BlockchainRequest),
    BlockHeaderHash(SaitoHash),
    Ping(),
    SPVChain(),
    Services(),
    GhostChain(),
    GhostChainRequest(),
    Result(),
    Error(),
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
            Message::HandshakeCompletion(data) => data.serialize(),
            Message::ApplicationMessage(data) => data.clone(),
            Message::Block(data) => data.serialize_for_net(BlockType::Full),
            Message::Transaction(data) => data.serialize_for_net(),
            Message::BlockchainRequest(data) => data.serialize(),
            Message::BlockHeaderHash(data) => data.to_vec(),
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
                return Ok(Message::HandshakeChallenge(result));
            }
            2 => {
                let result = HandshakeResponse::deserialize(&buffer)?;
                return Ok(Message::HandshakeResponse(result));
            }
            3 => {
                let result = HandshakeCompletion::deserialize(&buffer)?;
                return Ok(Message::HandshakeCompletion(result));
            }
            4 => {
                return Ok(Message::ApplicationMessage(buffer));
            }
            5 => {
                let block = Block::deserialize_from_net(&buffer);
                return Ok(Message::Block(block));
            }
            6 => {
                let tx = Transaction::deserialize_from_net(&buffer);
                return Ok(Message::Transaction(tx));
            }
            7 => {
                let result = BlockchainRequest::deserialize(&buffer)?;
                return Ok(Message::BlockchainRequest(result));
            }
            8 => {
                assert_eq!(buffer.len(), 32);
                let result = buffer[0..32].to_vec().try_into().unwrap();
                return Ok(Message::BlockHeaderHash(result));
            }
            9 => {
                return Ok(Message::Ping());
            }
            10 => return Ok(Message::SPVChain()),
            11 => return Ok(Message::Services()),
            12 => return Ok(Message::GhostChain()),
            13 => return Ok(Message::GhostChainRequest()),
            14 => return Ok(Message::Result()),
            15 => return Ok(Message::Error()),
            _ => {
                warn!("message type : {:?} not valid", message_type);
                return Err(Error::from(ErrorKind::InvalidData));
            }
        }
    }
    pub fn get_type_value(&self) -> u8 {
        match self {
            Message::HandshakeChallenge(_) => 1,
            Message::HandshakeResponse(_) => 2,
            Message::HandshakeCompletion(_) => 3,
            Message::ApplicationMessage(_) => 4,
            Message::Block(_) => 5,
            Message::Transaction(_) => 6,
            Message::BlockchainRequest(_) => 7,
            Message::BlockHeaderHash(_) => 8,
            Message::Ping() => 9,
            Message::SPVChain() => 10,
            Message::Services() => 11,
            Message::GhostChain() => 12,
            Message::GhostChainRequest() => 13,
            Message::Result() => 14,
            Message::Error() => 15,
        }
    }
}

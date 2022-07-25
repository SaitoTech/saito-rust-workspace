use std::io::{Error, ErrorKind};

use log::{debug, info, warn};

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
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let message_type: u8 = self.get_type_value();
        let internal_buffer = match self {
            Message::HandshakeChallenge(data) => data.serialize(),
            Message::HandshakeResponse(data) => data.serialize(),
            Message::HandshakeCompletion(data) => data.serialize(),
            Message::ApplicationMessage(data) => data.clone(),
            Message::Block(data) => data.serialize_for_net(BlockType::Full),
            Message::Transaction(data) => data.serialize_for_net(),
            Message::BlockchainRequest(data) => data.serialize(),
            Message::BlockHeaderHash(data) => data.to_vec(),
        };
        [vec![message_type], internal_buffer].concat()
    }
    pub fn deserialize(buffer: Vec<u8>) -> Result<Message, Error> {
        let message_type = buffer[0];
        let buffer = buffer[1..].to_vec();

        debug!("deserialize message : buffer size = {:?}", buffer.len());

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
                todo!()
            }
            5 => {
                todo!()
            }
            6 => {
                todo!()
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
        }
    }
}

use std::io::{Error, ErrorKind};

use log::{trace, warn};

use crate::common::defs::SaitoHash;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::msg::api_message::ApiMessage;
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{HandshakeChallenge, HandshakeResponse};
use crate::core::data::serialize::Serialize;
use crate::core::data::transaction::Transaction;

#[derive(Debug)]
pub enum Message {
    HandshakeChallenge(HandshakeChallenge),
    HandshakeResponse(HandshakeResponse),
    ApplicationMessage(ApiMessage),
    Block(Block),
    Transaction(Transaction),
    BlockchainRequest(BlockchainRequest),
    BlockHeaderHash(SaitoHash, u64),
    Ping(),
    SPVChain(),
    Services(),
    GhostChain(),
    GhostChainRequest(),
    Result(ApiMessage),
    Error(ApiMessage),
    // ApplicationTransaction(ApiMessage),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let message_type: u8 = self.get_type_value();
        let mut buffer: Vec<u8> = vec![];
        buffer.extend(&message_type.to_be_bytes());
        buffer.append(&mut match self {
            Message::HandshakeChallenge(data) => data.serialize(),
            Message::HandshakeResponse(data) => data.serialize(),
            Message::ApplicationMessage(data) => data.serialize(),
            // Message::ApplicationTransaction(data) => data.clone(),
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
        let buffer = buffer[1..].to_vec();

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

            3 => {
                let result = ApiMessage::deserialize(&buffer);
                Ok(Message::ApplicationMessage(result))
            }
            4 => {
                let block = Block::deserialize_from_net(buffer)?;
                Ok(Message::Block(block))
            }
            5 => {
                let tx = Transaction::deserialize_from_net(&buffer);
                Ok(Message::Transaction(tx))
            }
            6 => {
                let result = BlockchainRequest::deserialize(&buffer)?;
                Ok(Message::BlockchainRequest(result))
            }
            7 => {
                assert_eq!(buffer.len(), 40);
                let block_hash = buffer[0..32].to_vec().try_into().unwrap();
                let block_id = u64::from_be_bytes(buffer[32..40].to_vec().try_into().unwrap());
                Ok(Message::BlockHeaderHash(block_hash, block_id))
            }
            8 => Ok(Message::Ping()),
            9 => Ok(Message::SPVChain()),
            10 => Ok(Message::Services()),
            11 => Ok(Message::GhostChain()),
            12 => Ok(Message::GhostChainRequest()),
            13 => {
                let result = ApiMessage::deserialize(&buffer);
                Ok(Message::Result(result))
            }
            14 => {
                let result = ApiMessage::deserialize(&buffer);
                Ok(Message::Error(result))
            }
            // 16 => Ok(Message::ApplicationTransaction(buffer)),
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
            Message::ApplicationMessage(_) => 3,
            Message::Block(_) => 4,
            Message::Transaction(_) => 5,
            Message::BlockchainRequest(_) => 6,
            Message::BlockHeaderHash(_, _) => 7,
            Message::Ping() => 8,
            Message::SPVChain() => 9,
            Message::Services() => 10,
            Message::GhostChain() => 11,
            Message::GhostChainRequest() => 12,
            Message::Result(_) => 13,
            Message::Error(_) => 14,
            // Message::ApplicationTransaction(_) => 16,
        }
    }
}

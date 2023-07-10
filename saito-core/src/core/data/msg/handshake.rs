use std::io::{Error, ErrorKind};

use log::warn;

use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoSignature};
use crate::core::data::peer_service::PeerService;
use crate::core::data::serialize::Serialize;

#[derive(Debug)]
pub struct HandshakeChallenge {
    pub challenge: SaitoHash,
}

// TODO : can we drop other 2 structs and only use this ? need to confirm with more fields being added
#[derive(Debug)]
pub struct HandshakeResponse {
    pub public_key: SaitoPublicKey,
    pub signature: SaitoSignature,
    pub is_lite: bool,
    pub block_fetch_url: String,
    pub challenge: SaitoHash,
    pub services: Vec<PeerService>,
}

// #[derive(Debug)]
// pub struct HandshakeCompletion {
//     pub public_key: SaitoPublicKey,
//     pub is_lite: u64,
//     pub block_fetch_url: String,
//     pub signature: SaitoSignature,
// }

impl Serialize<Self> for HandshakeChallenge {
    fn serialize(&self) -> Vec<u8> {
        let buffer = [self.challenge.to_vec()].concat();
        return buffer;
    }
    fn deserialize(buffer: &Vec<u8>) -> Result<Self, Error> {
        if buffer.len() < 32 {
            warn!(
                "Deserializing Handshake Challenge, buffer size is :{:?}",
                buffer.len()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }

        let mut challenge = HandshakeChallenge { challenge: [0; 32] };
        challenge.challenge = buffer[0..32].to_vec().try_into().unwrap();

        Ok(challenge)
    }
}

impl Serialize<Self> for HandshakeResponse {
    fn serialize(&self) -> Vec<u8> {
        [
            self.public_key.to_vec(),
            self.signature.to_vec(),
            self.challenge.to_vec(),
            (self.is_lite as u8).to_be_bytes().to_vec(),
            (self.block_fetch_url.len() as u32).to_be_bytes().to_vec(),
            self.block_fetch_url.as_bytes().to_vec(),
            PeerService::serialize_services(&self.services),
        ]
        .concat()
    }
    fn deserialize(buffer: &Vec<u8>) -> Result<Self, Error> {
        if buffer.len() < 134 {
            warn!(
                "Deserializing failed for handshake response, buffer size is :{:?}",
                buffer.len()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }

        let mut response = HandshakeResponse {
            public_key: buffer[0..33].to_vec().try_into().unwrap(),
            signature: buffer[33..97].to_vec().try_into().unwrap(),
            challenge: buffer[97..129].to_vec().try_into().unwrap(),
            is_lite: buffer[129] != 0,
            block_fetch_url: "".to_string(),
            services: vec![],
        };
        let url_length = u32::from_be_bytes(buffer[130..134].try_into().unwrap());

        // if we detect a block fetch url, we will retrieve it
        if url_length > 0 {
            let result = String::from_utf8(buffer[134..(134 + url_length) as usize].to_vec());
            if result.is_err() {
                warn!(
                    "failed decoding block fetch url. {:?}",
                    result.err().unwrap()
                );
                return Err(Error::from(ErrorKind::InvalidData));
            }

            response.block_fetch_url = result.unwrap();
        }
        // if we detect services, we deserialize that too
        if buffer.len() > (134 + url_length) as usize {
            let service_buffer = buffer[(134 + url_length) as usize..].to_vec();

            let services = PeerService::deserialize_services(service_buffer)?;
            response.services = services;
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::msg::handshake::{HandshakeChallenge, HandshakeResponse};
    use crate::core::data::serialize::Serialize;

    #[test]
    fn test_handshake() {
        let crypto = secp256k1::Secp256k1::new();

        let (_secret_key_1, _public_key_1) =
            crypto.generate_keypair(&mut secp256k1::rand::thread_rng());
        let (secret_key_2, public_key_2) =
            crypto.generate_keypair(&mut secp256k1::rand::thread_rng());
        let challenge = HandshakeChallenge {
            challenge: rand::random(),
        };
        let buffer = challenge.serialize();
        assert_eq!(buffer.len(), 32);
        let challenge2 = HandshakeChallenge::deserialize(&buffer).expect("deserialization failed");
        assert_eq!(challenge.challenge, challenge2.challenge);

        let signature = crypto.sign_ecdsa(
            &secp256k1::Message::from_slice(&challenge.challenge).unwrap(),
            &secret_key_2,
        );
        let response = HandshakeResponse {
            public_key: public_key_2.serialize(),
            signature: signature.serialize_compact(),
            challenge: rand::random(),
            is_lite: false,
            block_fetch_url: "http://url/test2".to_string(),
            services: vec![],
        };
        let buffer = response.serialize();
        assert_eq!(buffer.len(), 150);
        let response2 = HandshakeResponse::deserialize(&buffer).expect("deserialization failed");
        assert_eq!(response.challenge, response2.challenge);
        assert_eq!(response.public_key, response2.public_key);
        assert_eq!(response.block_fetch_url, response2.block_fetch_url);

        assert_eq!(response.signature, response2.signature);
        // let response = HandshakeCompletion {
        //     signature: signature.serialize_compact(),
        // };
        // let buffer = response.serialize();
        // assert_eq!(buffer.len(), 64);
        // let response2 = HandshakeCompletion::deserialize(&buffer).expect("deserialization failed");
        // assert_eq!(response.signature, response2.signature);
    }
}

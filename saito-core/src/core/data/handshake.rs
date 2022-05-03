use crate::common::defs::{SaitoHash, SaitoPublicKey, SaitoSignature};

pub struct HandshakeChallenge {
    pub public_key: SaitoPublicKey,
    pub challenge: SaitoHash,
}

pub struct HandshakeResponse {
    pub public_key: SaitoPublicKey,
    pub signature: SaitoSignature,
    pub challenge: SaitoHash,
}

pub struct HandshakeCompletion {
    pub signature: SaitoSignature,
}

impl HandshakeChallenge {
    pub fn serialize(&self) -> Vec<u8> {
        let buffer = [self.public_key.to_vec(), self.challenge.to_vec()].concat();

        return buffer;
    }
    pub fn deserialize(buffer: &Vec<u8>) -> Self {
        let mut challenge = HandshakeChallenge {
            public_key: [0; 33],
            challenge: [0; 32],
        };
        challenge.public_key = buffer[0..33].to_vec().try_into().unwrap();
        challenge.challenge = buffer[33..65].to_vec().try_into().unwrap();
        return challenge;
    }
}

impl HandshakeResponse {
    pub fn serialize(&self) -> Vec<u8> {
        [
            self.public_key.to_vec(),
            self.signature.to_vec(),
            self.challenge.to_vec(),
        ]
        .concat()
    }
    pub fn deserialize(buffer: &Vec<u8>) -> Self {
        HandshakeResponse {
            public_key: buffer[0..33].to_vec().try_into().unwrap(),
            signature: buffer[33..97].to_vec().try_into().unwrap(),
            challenge: buffer[97..129].to_vec().try_into().unwrap(),
        }
    }
}

impl HandshakeCompletion {
    pub fn serialize(&self) -> Vec<u8> {
        self.signature.to_vec()
    }
    pub fn deserialize(buffer: &Vec<u8>) -> Self {
        HandshakeCompletion {
            signature: buffer[0..64].try_into().unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng};

    use crate::core::data::handshake::{
        HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
    };

    #[test]
    fn test_handshake() {
        let crypto = secp256k1::Secp256k1::new();

        let (secret_key_1, public_key_1) =
            crypto.generate_keypair(&mut secp256k1::rand::thread_rng());
        let (secret_key_2, public_key_2) =
            crypto.generate_keypair(&mut secp256k1::rand::thread_rng());
        let mut challenge = HandshakeChallenge {
            public_key: public_key_1.serialize(),
            challenge: rand::random(),
        };
        let buffer = challenge.serialize();
        assert_eq!(buffer.len(), 65);
        let challenge2 = HandshakeChallenge::deserialize(&buffer);
        assert_eq!(challenge.challenge, challenge2.challenge);
        assert_eq!(challenge.public_key, challenge2.public_key);

        let signature = crypto.sign(
            &secp256k1::Message::from_slice(&challenge.challenge).unwrap(),
            &secret_key_2,
        );
        let mut response = HandshakeResponse {
            public_key: public_key_2.serialize(),
            signature: signature.serialize_compact(),
            challenge: rand::random(),
        };
        let buffer = response.serialize();
        assert_eq!(buffer.len(), 129);
        let response2 = HandshakeResponse::deserialize(&buffer);
        assert_eq!(response.challenge, response2.challenge);
        assert_eq!(response.public_key, response2.public_key);

        assert_eq!(response.signature, response2.signature);
        let response = HandshakeCompletion {
            signature: signature.serialize_compact(),
        };
        let buffer = response.serialize();
        assert_eq!(buffer.len(), 64);
        let response2 = HandshakeCompletion::deserialize(&buffer);
        assert_eq!(response.signature, response2.signature);
    }
}

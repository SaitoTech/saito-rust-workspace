use ahash::AHashMap;

pub type Currency = u128;
pub type Signature = [u8; 64];
pub type SaitoPublicKey = [u8; 33];
pub type PrivateKey = [u8; 32];
pub type SaitoHash = [u8; 32];
pub type SaitoUTXOSetKey = [u8; 74];
pub type UtxoSet = AHashMap<SaitoUTXOSetKey, u64>;

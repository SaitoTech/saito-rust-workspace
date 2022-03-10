use crate::core::data::slip::Slip;

pub enum TransactionType {
    Normal,
    Fee,
    GoldenTicket,
    ATR,
    Vip,
    StakerDeposit,
    StakerWithdrawal,
    Issuance,
    SPV,
}

pub struct Transaction {
    timestamp: u64,
    pub inputs: Vec<Slip>,
    pub outputs: Vec<Slip>,
    #[serde(with = "serde_bytes")]
    message: Vec<u8>,
    transaction_type: TransactionType,
    #[serde_as(as = "[_; 64]")]
    signature: SaitoSignature,
    path: Vec<Hop>,
}

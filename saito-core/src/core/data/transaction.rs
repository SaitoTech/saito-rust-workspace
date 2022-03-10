use crate::common::defs::Signature;
use crate::core::data::hop::Hop;
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
    message: Vec<u8>,
    transaction_type: TransactionType,
    signature: Signature,
    path: Vec<Hop>,
}

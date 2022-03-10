use crate::common::defs::{Currency, Hash32, PublicKey};

pub enum SlipType {
    Normal,
    ATR,
    VipInput,
    VipOutput,
    MinerInput,
    MinerOutput,
    RouterInput,
    RouterOutput,
    StakerOutput,
    StakerDeposit,
    StakerWithdrawalPending,
    StakerWithdrawalStaking,
}

pub struct Slip {
    public_key: PublicKey,
    uuid: Hash32,
    amount: Currency,
    payout: Currency,
    slip_ordinal: u8,
    slip_type: SlipType,
}

impl Slip {}

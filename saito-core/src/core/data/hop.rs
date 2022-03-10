use crate::common::defs::{PublicKey, Signature};

pub struct Hop {
    from: PublicKey,
    to: PublicKey,
    sig: Signature,
}

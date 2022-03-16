use crate::common::defs::{SaitoPublicKey, Signature};

#[derive(Clone, Debug)]
pub struct Hop {
    from: SaitoPublicKey,
    to: SaitoPublicKey,
    sig: Signature,
}

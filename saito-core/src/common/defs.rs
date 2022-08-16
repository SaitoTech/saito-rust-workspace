use ahash::AHashMap;

#[cfg(feature = "locking-logs")]
use log::trace;

pub type Currency = u128;
pub type SaitoSignature = [u8; 64];
pub type SaitoPublicKey = [u8; 33];
pub type SaitoPrivateKey = [u8; 32];
pub type SaitoHash = [u8; 32];
pub type SaitoUTXOSetKey = [u8; 74];
pub type UtxoSet = AHashMap<SaitoUTXOSetKey, bool>;

pub const BLOCK_FILE_EXTENSION: &str = ".sai";

#[macro_export]
macro_rules! log_write_lock_request {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        trace!("waiting for {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_write_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        trace!("acquired {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_request {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        trace!("waiting for {:?} lock for reading", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        trace!("acquired {:?} lock for reading", $resource);
    };
}

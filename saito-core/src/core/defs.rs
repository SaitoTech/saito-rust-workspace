use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use ahash::AHashMap;
use tokio::sync::mpsc::Sender;

pub type Currency = u64;
/// Time in milliseconds
pub type Timestamp = u64;
pub type SaitoSignature = [u8; 64];
pub type SaitoPublicKey = [u8; 33];
pub type SaitoPrivateKey = [u8; 32];
pub type SaitoHash = [u8; 32];
// pub type SlipUuid = [u8; 17];
pub type SaitoUTXOSetKey = [u8; 58];
pub type UtxoSet = AHashMap<SaitoUTXOSetKey, bool>;
pub type PeerIndex = u64;
pub type BlockId = u64;

pub const NOLAN_PER_SAITO: Currency = 100_000_000;

pub const PROJECT_PUBLIC_KEY: &'static str = "q6TTBeSStCLXEPoS5TUVAxNiGGnRDZQenpvAXXAfTmtA";

#[cfg(test)]
// length of 1 genesis period
pub const GENESIS_PERIOD: u64 = 10;

#[cfg(not(test))]
pub const GENESIS_PERIOD: u64 = 100_000;

// prune blocks from index after N blocks
pub const PRUNE_AFTER_BLOCKS: u64 = 8;
// max recursion when paying stakers -- number of blocks including  -- number of blocks including GTT
pub const MAX_STAKER_RECURSION: u64 = 3;
// max token supply - used in validating block #1
pub const MAX_TOKEN_SUPPLY: Currency = 1_000_000_000_000_000_000;
// minimum golden tickets required ( NUMBER_OF_TICKETS / number of preceding blocks )
pub const MIN_GOLDEN_TICKETS_NUMERATOR: u64 = 2;
// minimum golden tickets required ( number of tickets / NUMBER_OF_PRECEDING_BLOCKS )
pub const MIN_GOLDEN_TICKETS_DENOMINATOR: u64 = 6;

pub const BLOCK_FILE_EXTENSION: &str = ".sai";
pub const STAT_BIN_COUNT: usize = 3;

pub const PEER_RECONNECT_WAIT_PERIOD: Timestamp = Duration::from_secs(10).as_millis() as Timestamp;
pub const WS_KEEP_ALIVE_PERIOD: Timestamp = Duration::from_secs(10).as_millis() as Timestamp;

/// NOTE : Lock ordering is decided from how frequent the usage is for that resource. Please make sure to follow the order given below to avoid deadlocks
/// network controller
/// sockets
/// configs
/// blockchain
/// mempool
/// peers
/// wallet
/// TODO : add a macro to check the lock ordering as a feature flag
///

pub const LOCK_ORDER_NETWORK_CONTROLLER: u8 = 1;
pub const LOCK_ORDER_SOCKETS: u8 = 2;
pub const LOCK_ORDER_CONFIGS: u8 = 3;
pub const LOCK_ORDER_BLOCKCHAIN: u8 = 4;
pub const LOCK_ORDER_MEMPOOL: u8 = 5;
pub const LOCK_ORDER_PEERS: u8 = 6;
pub const LOCK_ORDER_WALLET: u8 = 7;

thread_local! {
    pub static LOCK_ORDER: RefCell<VecDeque<u8>> = RefCell::new(VecDeque::default());
}

pub struct LockGuardWatcher {
    order: u8,
}

impl Drop for LockGuardWatcher {
    fn drop(&mut self) {
        #[cfg(feature = "locking-logs")]
        LOCK_ORDER.with(|v| {
            let mut v = v.borrow_mut();
            let res = v.pop_back();
            // println!("releasing lock : {:?}", self.order);
            assert!(
                res.is_some(),
                "no existing locks found for lock : {:?}",
                self.order
            );
            let r = res.unwrap();
            assert_eq!(
                self.order, r,
                "not the expected lock : {:?} vs actual : {:?}",
                self.order, r
            );
        });
    }
}

// pub fn push_lock(order: u8) -> LockGuardWatcher {
//     #[cfg(feature = "locking-logs")]
//     LOCK_ORDER.with(|v| {
//         let mut v = v.borrow_mut();
//         let res = v.back();
//         if let Some(res) = res {
//             assert!(
//                 *res < order,
//                 "lock : {:?} cannot be locked after : {:?}",
//                 order,
//                 *res
//             );
//         }
//         // println!("locking : {:?}", order);
//         v.push_back(order);
//     });
//     LockGuardWatcher { order }
// }

#[macro_export]
macro_rules! lock_for_write {
    ($lock:expr, $order:expr) => {{
        let l = $lock.write().await;
        l
    }};
}

#[macro_export]
macro_rules! lock_for_read {
    ($lock:expr, $order:expr) => {{
        let l = $lock.read().await;
        l
    }};
}

#[macro_export]
macro_rules! iterate {
    ($collection:expr, $min:expr) => {{
        #[cfg(feature = "with-rayon")]
        {
            $collection.par_iter().with_min_len($min)
        }

        #[cfg(not(feature = "with-rayon"))]
        {
            $collection.iter()
        }
    }};
}

#[macro_export]
macro_rules! iterate_mut {
    ($collection:expr) => {{
        #[cfg(feature = "with-rayon")]
        {
            $collection.par_iter_mut()
        }

        #[cfg(not(feature = "with-rayon"))]
        {
            $collection.iter_mut()
        }
    }};
}

#[macro_export]
macro_rules! drain {
    ($collection:expr, $min:expr) => {{
        #[cfg(feature = "with-rayon")]
        {
            $collection.par_drain(..).with_min_len($min)
        }

        #[cfg(not(feature = "with-rayon"))]
        {
            $collection.drain(..)
        }
    }};
}

#[derive(Clone, Debug)]
pub struct StatVariable {
    pub total: u64,
    pub count_since_last_stat: u64,
    pub last_stat_at: Timestamp,
    pub bins: VecDeque<(u64, Timestamp)>,
    pub avg: f64,
    pub max_avg: f64,
    pub min_avg: f64,
    pub name: String,
    pub sender: Sender<String>,
}

impl StatVariable {
    pub fn new(name: String, bin_count: usize, sender: Sender<String>) -> StatVariable {
        StatVariable {
            total: 0,
            count_since_last_stat: 0,
            last_stat_at: 0,
            bins: VecDeque::with_capacity(bin_count),
            avg: 0.0,
            max_avg: 0.0,
            min_avg: f64::MAX,
            name,
            sender,
        }
    }
    pub fn increment(&mut self) {
        #[cfg(feature = "with-stats")]
        {
            self.total += 1;
            self.count_since_last_stat += 1;
        }
    }
    pub fn increment_by(&mut self, amount: u64) {
        #[cfg(feature = "with-stats")]
        {
            self.total += amount;
            self.count_since_last_stat += amount;
        }
    }
    pub async fn calculate_stats(&mut self, current_time_in_ms: Timestamp) {
        let time_elapsed_in_ms = current_time_in_ms - self.last_stat_at;
        self.last_stat_at = current_time_in_ms;
        if self.bins.len() == self.bins.capacity() - 1 {
            self.bins.pop_front();
        }
        self.bins
            .push_back((self.count_since_last_stat, time_elapsed_in_ms));
        self.count_since_last_stat = 0;

        let mut total = 0;
        let mut total_time_in_ms = 0;
        for (count, time) in self.bins.iter() {
            total += *count;
            total_time_in_ms += *time;
        }

        self.avg = (1_000.0 * total as f64) / total_time_in_ms as f64;
        if self.avg > self.max_avg {
            self.max_avg = self.avg;
        }
        if self.avg < self.min_avg {
            self.min_avg = self.avg;
        }
        #[cfg(feature = "with-stats")]
        self.sender
            .send(self.print(current_time_in_ms))
            .await
            .expect("failed sending stat update");
    }

    fn print(&self, current_time_in_ms: Timestamp) -> String {
        format!(
            // target : "saito_stats",
            "{} - {} - total : {:?}, current_rate : {:.2}, max_rate : {:.2}, min_rate : {:.2}",
            current_time_in_ms,
            format!("{:width$}", self.name, width = 40),
            self.total,
            self.avg,
            self.max_avg,
            self.min_avg
        )
    }
}

pub trait PrintForLog<T: TryFrom<Vec<u8>>> {
    fn to_base58(&self) -> String;
    fn to_hex(&self) -> String;
    fn from_hex(str: &str) -> Result<T, String>;

    fn from_base58(str: &str) -> Result<T, String>;
}

#[macro_export]
macro_rules! impl_print {
    ($st:ident) => {
        impl PrintForLog<$st> for $st {
            fn to_base58(&self) -> String {
                bs58::encode(self).into_string()
            }

            fn to_hex(&self) -> String {
                hex::encode(self)
            }

            fn from_hex(str: &str) -> Result<$st, String> {
                let result = hex::decode(str);
                if result.is_err() {
                    return Err(format!(
                        "couldn't convert string : {:?} to hex type. {:?}",
                        str,
                        result.err().unwrap()
                    ));
                }
                let result = result.unwrap();
                let result = result.try_into();
                if result.is_err() {
                    return Err(format!(
                        "couldn't convert : {:?} with length : {:?} to hex type. {:?}",
                        str,
                        str.len(),
                        result.err().unwrap()
                    ));
                }
                Ok(result.unwrap())
            }
            fn from_base58(str: &str) -> Result<$st, String> {
                let result = bs58::decode(str).into_vec();
                if result.is_err() {
                    return Err(format!(
                        "couldn't convert string : {:?} to base58. {:?}",
                        str,
                        result.err().unwrap()
                    ));
                }
                let result = result.unwrap();
                let result = result.try_into();
                if result.is_err() {
                    return Err(format!(
                        "couldn't convert : {:?} with length : {:?} to base58. {:?}",
                        str,
                        str.len(),
                        result.err().unwrap()
                    ));
                }
                Ok(result.unwrap())
            }
        }
    };
}
impl_print!(SaitoHash);
impl_print!(SaitoPublicKey);
impl_print!(SaitoSignature);
impl_print!(SaitoUTXOSetKey);

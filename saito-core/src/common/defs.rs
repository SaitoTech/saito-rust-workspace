use ahash::AHashMap;
use std::collections::VecDeque;
use std::time::Duration;

pub type Currency = u128;
pub type Timestamp = u64;
pub type SaitoSignature = [u8; 64];
pub type SaitoPublicKey = [u8; 33];
pub type SaitoPrivateKey = [u8; 32];
pub type SaitoHash = [u8; 32];
pub type SlipUuid = [u8; 17];
pub type SaitoUTXOSetKey = [u8; 58];
pub type UtxoSet = AHashMap<SaitoUTXOSetKey, bool>;

pub const BLOCK_FILE_EXTENSION: &str = ".sai";
pub const STAT_INTERVAL: Timestamp = Duration::from_secs(5).as_micros() as Timestamp;
pub const STAT_BIN_COUNT: usize = 10;

// TODO : these should be configurable
pub const CHANNEL_SIZE: usize = 1000_000;
pub const STAT_TIMER: Duration = Duration::from_secs(5);
pub const THREAD_SLEEP_TIME: Duration = Duration::from_millis(50);

/// NOTE : Lock ordering is decided from how frequent the usage is for that resource. Please make sure to follow the order given below to avoid deadlocks
/// network controller
/// sockets
/// configs
/// blockchain
/// mempool
/// peers
/// wallet
/// TODO : add a macro to check the lock ordering as a feature flag
#[macro_export]
macro_rules! log_write_lock_request {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        tracing::debug!("waiting for {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_write_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        tracing::debug!("acquired {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_request {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        tracing::debug!("waiting for {:?} lock for reading", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        tracing::debug!("acquired {:?} lock for reading", $resource);
    };
}

#[macro_export]
macro_rules! stat {
    ($resource:expr) => {
        #[cfg(feature = "with-stats")]
        tracing::info!("{:?}", $resource);
    };
}

pub struct StatVariable {
    pub total: u64,
    pub count_since_last_stat: u64,
    pub last_stat_at: Timestamp,
    pub bins: VecDeque<(u64, Timestamp)>,
    pub avg: f64,
    pub max_avg: f64,
    pub min_avg: f64,
    pub name: String,
}

impl StatVariable {
    pub fn new(name: String, bin_count: usize) -> StatVariable {
        StatVariable {
            total: 0,
            count_since_last_stat: 0,
            last_stat_at: 0,
            bins: VecDeque::with_capacity(bin_count),
            avg: 0.0,
            max_avg: 0.0,
            min_avg: f64::MAX,
            name,
        }
    }
    pub fn increment(&mut self) {
        #[cfg(feature = "with-stats")]
        {
            self.total += 1;
            self.count_since_last_stat += 1;
        }
    }
    pub fn calculate_stats(&mut self, current_time_in_us: Timestamp) {
        let time_elapsed_in_us = current_time_in_us - self.last_stat_at;
        self.last_stat_at = current_time_in_us;
        if self.bins.len() == self.bins.capacity() - 1 {
            self.bins.pop_front();
        }
        self.bins
            .push_back((self.count_since_last_stat, time_elapsed_in_us));
        self.count_since_last_stat = 0;

        let mut total = 0;
        let mut total_time_in_us = 0;
        for (count, time) in self.bins.iter() {
            total += *count;
            total_time_in_us += *time;
        }

        self.avg = (1_000_000.0 * total as f64) / total_time_in_us as f64;
        if self.avg > self.max_avg {
            self.max_avg = self.avg;
        }
        if self.avg < self.min_avg {
            self.min_avg = self.avg;
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub fn print(&self) {
        #[cfg(feature = "with-stats")]
        {
            println!(
                // target : "saito_stats",
                "--- stats ------ {:?} - total : {:?} current_rate : {:.5} max_rate : {:.5} min_rate : {:.5}",
                self.name.as_str(),
                self.total,
                self.avg,
                self.max_avg,
                self.min_avg
            );
        }
    }
}

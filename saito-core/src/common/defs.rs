use std::collections::VecDeque;

use ahash::AHashMap;
use tokio::sync::mpsc::Sender;

pub type Currency = u128;
pub type Timestamp = u64;
pub type SaitoSignature = [u8; 64];
pub type SaitoPublicKey = [u8; 33];
pub type SaitoPrivateKey = [u8; 32];
pub type SaitoHash = [u8; 32];
// pub type SlipUuid = [u8; 17];
pub type SaitoUTXOSetKey = [u8; 66];
pub type UtxoSet = AHashMap<SaitoUTXOSetKey, bool>;
pub type PeerIndex = u64;
pub type BlockId = u64;

pub const BLOCK_FILE_EXTENSION: &str = ".sai";
pub const STAT_BIN_COUNT: usize = 3;

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
        println!("waiting for {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_write_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        println!("acquired {:?} lock for writing", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_request {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        println!("waiting for {:?} lock for reading", $resource);
    };
}

#[macro_export]
macro_rules! log_read_lock_receive {
    ($resource: expr) => {
        #[cfg(feature = "locking-logs")]
        println!("acquired {:?} lock for reading", $resource);
    };
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
            .send(self.print())
            .await
            .expect("failed sending stat update");
    }

    #[tracing::instrument(level = "info", skip_all)]
    fn print(&self) -> String {
        format!(
            // target : "saito_stats",
            "--- stats ------ {} - total : {:?} current_rate : {:.2} max_rate : {:.2} min_rate : {:.2}",
            format!("{:width$}", self.name, width = 30),
            self.total,
            self.avg,
            self.max_avg,
            self.min_avg
        )
    }
}

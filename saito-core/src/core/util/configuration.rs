use std::fmt::Debug;

use crate::core::defs::Timestamp;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct PeerConfig {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub synctype: String,
    #[serde(skip)]
    pub is_main: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub protocol: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Server {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub protocol: String,
    pub endpoint: Endpoint,
    #[serde(default)]
    pub verification_threads: u16,
    #[serde(default)]
    pub channel_size: u64,
    #[serde(default)]
    pub stat_timer_in_ms: u64,
    #[serde(default)]
    pub thread_sleep_time_in_ms: u64,
    #[serde(default)]
    pub block_fetch_batch_size: u64,
    #[serde(default)]
    pub reconnection_wait_time: Timestamp,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BlockchainConfig {
    #[serde(default)]
    pub last_block_hash: String,
    #[serde(default)]
    pub last_block_id: u64,
    #[serde(default)]
    pub last_timestamp: u64,
    #[serde(default)]
    pub genesis_block_id: u64,
    #[serde(default)]
    pub genesis_timestamp: u64,
    #[serde(default)]
    pub lowest_acceptable_timestamp: u64,
    #[serde(default)]
    pub lowest_acceptable_block_hash: String,
    #[serde(default)]
    pub lowest_acceptable_block_id: u64,
    #[serde(default)]
    pub fork_id: String,
}

pub trait Configuration: Debug {
    fn get_server_configs(&self) -> Option<&Server>;
    fn get_peer_configs(&self) -> &Vec<PeerConfig>;
    fn get_blockchain_configs(&self) -> Option<BlockchainConfig>;
    fn get_block_fetch_url(&self) -> String;
    fn is_spv_mode(&self) -> bool;
    fn is_browser(&self) -> bool;
    fn replace(&mut self, config: &dyn Configuration);
}

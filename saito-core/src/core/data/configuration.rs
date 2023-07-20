use std::fmt::Debug;

use serde::Deserialize;
use serde::Serialize;

use crate::common::defs::Timestamp;

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
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub endpoint: Endpoint,
    pub verification_threads: u16,
    pub channel_size: u64,
    pub stat_timer_in_ms: u64,
    pub thread_sleep_time_in_ms: u64,
    pub block_fetch_batch_size: u64,
    pub reconnection_wait_time: Timestamp,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BlockchainConfig {
    pub last_block_hash: String,
    pub last_block_id: u64,
    pub last_timestamp: u64,
    pub genesis_block_id: u64,
    pub genesis_timestamp: u64,
    pub lowest_acceptable_timestamp: u64,
    pub lowest_acceptable_block_hash: String,
    pub lowest_acceptable_block_id: u64,
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

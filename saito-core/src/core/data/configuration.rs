use std::fmt::Debug;

use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
pub struct PeerConfig {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub synctype: String,
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
}

pub trait Configuration: Debug {
    fn get_server_configs(&self) -> Option<&Server>;
    fn get_peer_configs(&self) -> &Vec<PeerConfig>;
    fn get_block_fetch_url(&self) -> String;
    fn is_spv_mode(&self) -> bool;
    fn is_browser(&self) -> bool;
    fn replace(&mut self, config: &dyn Configuration);
}

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
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
}

pub trait Configuration {
    fn get_server_configs(&self) -> &Server;
    fn get_peer_configs(&self) -> &Vec<PeerConfig>;
    fn get_block_fetch_url(&self) -> String;
}

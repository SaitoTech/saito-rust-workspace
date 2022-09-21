use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};
use serde::Deserialize;
use tracing::{debug, error};

#[derive(Deserialize, Debug, Clone)]
pub struct Spammer {
    pub timer_in_milli: u64,
    pub burst_count: u32,
    pub tx_size: u32,
    pub tx_count: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpammerConfigs {
    server: Server,
    peers: Vec<PeerConfig>,
    spammer: Spammer,
}

impl SpammerConfigs {
    pub fn new() -> SpammerConfigs {
        SpammerConfigs {
            server: Server {
                host: "127.0.0.1".to_string(),
                port: 12100,
                protocol: "http".to_string(),
                endpoint: Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 12101,
                    protocol: "http".to_string(),
                },
            },
            peers: vec![],
            spammer: Spammer {
                timer_in_milli: 0,
                burst_count: 0,
                tx_size: 0,
                tx_count: 0,
            },
        }
    }

    pub fn get_spammer_configs(&self) -> &Spammer {
        return &self.spammer;
    }
}

impl Configuration for SpammerConfigs {
    fn get_server_configs(&self) -> &Server {
        return &self.server;
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        return &self.peers;
    }

    fn get_block_fetch_url(&self) -> String {
        return "".to_string();
    }
}

pub struct ConfigHandler {}

impl ConfigHandler {
    pub fn load_configs(config_file_path: String) -> Result<SpammerConfigs, Error> {
        debug!(
            "loading configurations from path : {:?} current_dir = {:?}",
            config_file_path,
            std::env::current_dir()
        );
        // TODO : add prompt with user friendly format
        let configs = Figment::new()
            .merge(Json::file(config_file_path))
            .extract::<SpammerConfigs>();

        if configs.is_err() {
            error!("{:?}", configs.err().unwrap());
            return Err(std::io::Error::from(ErrorKind::InvalidInput));
        }

        Ok(configs.unwrap())
    }
}

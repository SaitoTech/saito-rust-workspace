use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};
use serde::Deserialize;
use tracing::{debug, error};

#[derive(Deserialize, Debug, Clone)]
pub struct SpammerData {
    pub txs_per_second: u64,
    pub total_txs: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpammerConfigs {
    server: Server,
    peers: Vec<PeerConfig>,
    pub spammer: SpammerData,
}

impl Configuration for SpammerConfigs {
    fn get_server_configs(&self) -> &Server {
        &self.server
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        &self.peers
    }

    fn get_block_fetch_url(&self) -> String {
        let endpoint = &self.get_server_configs().endpoint;
        endpoint.protocol.to_string()
            + "://"
            + endpoint.host.as_str()
            + ":"
            + endpoint.port.to_string().as_str()
            + "/block/"
    }
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
            spammer: SpammerData {
                txs_per_second: 1000,
                total_txs: 1_000_000,
            },
        }
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

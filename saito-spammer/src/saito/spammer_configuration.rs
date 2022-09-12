use figment::providers::{Format, Json};
use figment::Figment;
use log::{debug, error};
use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};
use serde::Deserialize;
use std::io::{Error, ErrorKind};

#[derive(Deserialize, Debug)]
pub struct Spammer {
    pub timer_in_milli: u64,
    pub burst_count: u32,
    pub bytes_per_tx: u32,
}

#[derive(Deserialize, Debug)]
pub struct SpammerConfiguration {
    server: Server,
    peers: Vec<PeerConfig>,
    spammer: Spammer,
}

impl SpammerConfiguration {
    pub fn new() -> SpammerConfiguration {
        SpammerConfiguration {
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
                bytes_per_tx: 0,
            },
        }
    }

    pub fn get_spammer_configs(&self) -> &Spammer {
        return &self.spammer;
    }

    pub fn load_configs(config_file_path: String) -> Result<SpammerConfiguration, Error> {
        debug!(
            "loading configurations from path : {:?} current_dir = {:?}",
            config_file_path,
            std::env::current_dir()
        );

        let configs = Figment::new()
            .merge(Json::file(config_file_path))
            .extract::<SpammerConfiguration>();

        if configs.is_err() {
            error!("{:?}", configs.err().unwrap());
            return Err(std::io::Error::from(ErrorKind::InvalidInput));
        }

        Ok(configs.unwrap())
    }
}

impl Configuration for SpammerConfiguration {
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

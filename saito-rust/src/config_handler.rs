use figment::providers::{Format, Json};
use figment::Figment;
use log::{debug, error};
use saito_core::core::util::configuration::{
    BlockchainConfig, Configuration, Endpoint, PeerConfig, Server,
};
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind};
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Deserialize, Debug, Serialize)]
pub struct NodeConfigurations {
    server: Server,
    peers: Vec<PeerConfig>,
    #[serde(skip)]
    lite: bool,
    spv_mode: Option<bool>,
}

impl NodeConfigurations {
    pub fn write_to_file(&self, config_file_path: String) -> Result<(), Error> {
        let file = std::fs::File::create(config_file_path)?;
        serde_json::to_writer_pretty(&file, &self)?;
        Ok(())
    }
}
impl Default for NodeConfigurations {
    fn default() -> Self {
        NodeConfigurations {
            server: Server {
                host: "127.0.0.1".to_string(),
                port: 12101,
                protocol: "http".to_string(),
                endpoint: Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 12101,
                    protocol: "http".to_string(),
                },
                verification_threads: 4,
                channel_size: 1000,
                stat_timer_in_ms: 5000,
                thread_sleep_time_in_ms: 10,
                block_fetch_batch_size: 10,
                reconnection_wait_time: 10,
            },
            peers: vec![],
            lite: false,
            spv_mode: Some(false),
        }
    }
}

impl Configuration for NodeConfigurations {
    fn get_server_configs(&self) -> Option<&Server> {
        Some(&self.server)
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        &self.peers
    }

    fn get_blockchain_configs(&self) -> Option<BlockchainConfig> {
        None
    }

    fn get_block_fetch_url(&self) -> String {
        let endpoint = &self.get_server_configs().unwrap().endpoint;
        endpoint.protocol.to_string()
            + "://"
            + endpoint.host.as_str()
            + ":"
            + endpoint.port.to_string().as_str()
    }

    fn is_spv_mode(&self) -> bool {
        self.spv_mode.is_some() && self.spv_mode.unwrap()
    }

    fn is_browser(&self) -> bool {
        false
    }

    fn replace(&mut self, config: &dyn Configuration) {
        self.server = config.get_server_configs().cloned().unwrap();
        self.peers = config.get_peer_configs().clone();
        self.spv_mode = Some(config.is_spv_mode());
        self.lite = config.is_spv_mode();
    }
}

pub struct ConfigHandler {}

impl ConfigHandler {
    pub fn load_configs(config_file_path: String) -> Result<NodeConfigurations, Error> {
        debug!(
            "loading configurations from path : {:?} current_dir = {:?}",
            config_file_path,
            std::env::current_dir()
        );
        let path = Path::new(config_file_path.as_str());
        if !path.exists() {
            if path.parent().is_some() {
                std::fs::create_dir_all(path.parent().unwrap())?;
            }
            let configs = NodeConfigurations::default();
            configs.write_to_file(config_file_path.to_string())?;
        }
        // TODO : add prompt with user friendly format
        let configs = Figment::new()
            .merge(Json::file(config_file_path))
            .extract::<NodeConfigurations>();

        if configs.is_err() {
            error!("failed loading configs. {:?}", configs.err().unwrap());
            return Err(std::io::Error::from(ErrorKind::InvalidInput));
        }

        Ok(configs.unwrap())
    }
}

#[cfg(test)]
mod test {
    use std::io::ErrorKind;

    use saito_core::core::util::configuration::Configuration;

    use crate::config_handler::ConfigHandler;

    #[test]
    fn load_config_from_existing_file() {
        let path = String::from("src/test/data/config_handler_tests.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(
            configs.get_server_configs().unwrap().host,
            String::from("localhost")
        );
        assert_eq!(configs.get_server_configs().unwrap().port, 12101);
        assert_eq!(
            configs.get_server_configs().unwrap().protocol,
            String::from("http")
        );
        assert_eq!(
            configs.get_server_configs().unwrap().endpoint.host,
            String::from("localhost")
        );
        assert_eq!(configs.get_server_configs().unwrap().endpoint.port, 12101);
        assert_eq!(
            configs.get_server_configs().unwrap().endpoint.protocol,
            String::from("http")
        );
    }

    #[test]
    fn load_config_from_bad_file_format() {
        let path = String::from("src/test/data/config_handler_tests_bad_format.xml");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn load_config_from_non_existing_file() {
        let path = String::from("config/new_file_to_write.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_ok());
    }
}

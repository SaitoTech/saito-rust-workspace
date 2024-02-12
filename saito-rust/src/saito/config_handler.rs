use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use serde::Deserialize;

use log::{debug, error};
use saito_core::core::util::configuration::{
    BlockchainConfig, Configuration, Endpoint, PeerConfig, Server,
};

#[derive(Deserialize, Debug)]
pub struct NodeConfigurations {
    server: Server,
    peers: Vec<PeerConfig>,
    #[serde(skip)]
    lite: bool,
}

impl NodeConfigurations {}
impl Default for NodeConfigurations {
    fn default() -> Self {
        NodeConfigurations {
            server: Server {
                host: "".to_string(),
                port: 0,
                protocol: "".to_string(),
                endpoint: Endpoint {
                    host: "".to_string(),
                    port: 0,
                    protocol: "".to_string(),
                },
                verification_threads: 0,
                channel_size: 0,
                stat_timer_in_ms: 0,
                thread_sleep_time_in_ms: 0,
                block_fetch_batch_size: 0,
                reconnection_wait_time: 0,
            },
            peers: vec![],
            lite: false,
        }
    }
}

impl Configuration for NodeConfigurations {
    fn get_server_configs(&self) -> Option<&Server> {
        return Some(&self.server);
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        return &self.peers;
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
        false
    }

    fn is_browser(&self) -> bool {
        false
    }

    fn replace(&mut self, config: &dyn Configuration) {
        self.server = config.get_server_configs().cloned().unwrap();
        self.peers = config.get_peer_configs().clone();
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
        // TODO : add prompt with user friendly format
        let configs = Figment::new()
            .merge(Json::file(config_file_path))
            .extract::<NodeConfigurations>();

        if configs.is_err() {
            error!("{:?}", configs.err().unwrap());
            return Err(std::io::Error::from(ErrorKind::InvalidInput));
        }

        Ok(configs.unwrap())
    }
}

#[cfg(test)]
mod test {
    use std::io::ErrorKind;

    use crate::saito::config_handler::ConfigHandler;
    use saito_core::core::util::configuration::Configuration;

    #[test]
    fn load_config_from_existing_file() {
        let path = String::from("saito-rust/src/test/data/config_handler_tests.json");
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
        let path = String::from("saito-rust/src/test/data/config_handler_tests_bad_format.xml");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn load_config_from_non_existing_file() {
        let path = String::from("badfilename.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), ErrorKind::InvalidInput);
    }
}

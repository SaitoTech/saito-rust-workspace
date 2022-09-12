use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use tracing::{debug, error};
use log::{debug, error};
use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct NodeConfigurations {
    server: Server,
    peers: Vec<PeerConfig>,
}

impl NodeConfigurations {
    pub fn new() -> NodeConfigurations {
        NodeConfigurations {
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
        }
    }
}

impl Configuration for NodeConfigurations {
    fn get_server_configs(&self) -> &Server {
        return &self.server;
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        return &self.peers;
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
    use crate::ConfigHandler;
    use saito_core::core::data::configuration::Configuration;
    use std::io::ErrorKind;

    #[test]
    fn load_config_from_existing_file() {
        let path = String::from("saito-rust/src/test/data/config_handler_tests.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(configs.get_server_configs().host, String::from("localhost"));
        assert_eq!(configs.get_server_configs().port, 12101);
        assert_eq!(configs.get_server_configs().protocol, String::from("http"));
        assert_eq!(
            configs.get_server_configs().endpoint.host,
            String::from("localhost")
        );
        assert_eq!(configs.get_server_configs().endpoint.port, 12101);
        assert_eq!(
            configs.get_server_configs().endpoint.protocol,
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

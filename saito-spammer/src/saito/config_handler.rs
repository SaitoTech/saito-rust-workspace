use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use tracing::{debug, error};

use saito_core::core::data::configuration::Configuration;

pub struct ConfigHandler {}

impl ConfigHandler {
    pub fn load_configs(config_file_path: String) -> Result<Configuration, Error> {
        debug!(
            "loading configurations from path : {:?} current_dir = {:?}",
            config_file_path,
            std::env::current_dir()
        );
        // TODO : add prompt with user friendly format
        let configs = Figment::new()
            .merge(Json::file(config_file_path))
            .extract::<Configuration>();

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
    use std::io::ErrorKind;

    #[test]
    fn load_config_from_existing_file() {
        let path = String::from("saito-rust/src/test/data/config_handler_tests.json");
        let result = ConfigHandler::load_configs(path);
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(configs.server.host, String::from("localhost"));
        assert_eq!(configs.server.port, 12101);
        assert_eq!(configs.server.protocol, String::from("http"));
        assert_eq!(configs.server.endpoint.host, String::from("localhost"));
        assert_eq!(configs.server.endpoint.port, 12101);
        assert_eq!(configs.server.endpoint.protocol, String::from("http"));
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

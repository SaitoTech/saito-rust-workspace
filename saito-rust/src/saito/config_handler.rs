use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use log::{debug, error};

use saito_core::core::data::configuration::Configuration;

pub struct ConfigHandler {}

impl ConfigHandler {
    pub fn load_configs(config_file_path: String) -> Result<Configuration, Error> {
        debug!(
            "loading configurations from path : {:?} current_dir = {:?}",
            config_file_path,
            std::env::current_dir()
        );

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

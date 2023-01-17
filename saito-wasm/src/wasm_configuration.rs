use figment::providers::{Format, Json};
use figment::Figment;
use log::error;
use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};
use serde::Deserialize;
use std::io::{Error, ErrorKind};

// #[wasm_bindgen]
#[derive(Deserialize, Debug)]
pub struct WasmConfiguration {
    server: Server,
    peers: Vec<PeerConfig>,
    #[serde(skip)]
    lite: bool,
}

// #[wasm_bindgen]
impl WasmConfiguration {
    pub fn new() -> WasmConfiguration {
        WasmConfiguration {
            server: Server {
                host: "127.0.0.1".to_string(),
                port: 12100,
                protocol: "http".to_string(),
                endpoint: Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 12101,
                    protocol: "http".to_string(),
                },
                verification_threads: 2,
                channel_size: 1000,
                stat_timer_in_ms: 10000,
                thread_sleep_time_in_ms: 10,
                block_fetch_batch_size: 0,
            },
            peers: vec![],
            lite: false,
        }
    }
    pub fn new_from_json(json: &str) -> Result<WasmConfiguration, std::io::Error> {
        let configs = Figment::new()
            .merge(Json::string(json))
            .extract::<WasmConfiguration>();
        if configs.is_err() {
            error!(
                "failed parsing json string to configs. {:?}",
                configs.err().unwrap()
            );
            return Err(Error::from(ErrorKind::InvalidInput));
        }
        let configs = configs.unwrap();
        Ok(configs)
    }
}

impl Configuration for WasmConfiguration {
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
    fn is_lite(&self) -> bool {
        self.lite
    }
}

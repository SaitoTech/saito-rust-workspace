use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use log::error;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use saito_core::core::util::configuration::{
    BlockchainConfig, Configuration, Endpoint, PeerConfig, Server,
};

#[wasm_bindgen]
#[derive(Deserialize, Debug)]
pub struct WasmConfiguration {
    server: Option<Server>,
    peers: Vec<PeerConfig>,
    blockchain: Option<BlockchainConfig>,
    spv_mode: bool,
    browser_mode: bool,
}

#[wasm_bindgen]
impl WasmConfiguration {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmConfiguration {
        WasmConfiguration {
            server: Option::Some(Server {
                host: "localhost".to_string(),
                port: 12100,
                protocol: "http".to_string(),
                endpoint: Endpoint {
                    host: "localhost".to_string(),
                    port: 12101,
                    protocol: "http".to_string(),
                },
                verification_threads: 2,
                channel_size: 1000,
                stat_timer_in_ms: 10000,
                reconnection_wait_time: 10000,
                thread_sleep_time_in_ms: 10,
                block_fetch_batch_size: 0,
            }),
            peers: vec![],
            blockchain: None,
            spv_mode: false,
            browser_mode: false,
        }
    }
}

impl WasmConfiguration {
    pub fn new_from_json(json: &str) -> Result<WasmConfiguration, std::io::Error> {
        // info!("new from json : {:?}", json);
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
    fn get_server_configs(&self) -> Option<&Server> {
        return self.server.as_ref();
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        return &self.peers;
    }

    fn get_blockchain_configs(&self) -> Option<BlockchainConfig> {
        self.blockchain.clone()
    }

    fn get_block_fetch_url(&self) -> String {
        if self.get_server_configs().is_none() {
            return "".to_string();
        }
        let endpoint = &self.get_server_configs().unwrap().endpoint;
        endpoint.protocol.to_string()
            + "://"
            + endpoint.host.as_str()
            + ":"
            + endpoint.port.to_string().as_str()
    }
    fn is_spv_mode(&self) -> bool {
        self.spv_mode
    }

    fn is_browser(&self) -> bool {
        self.browser_mode
    }

    fn replace(&mut self, config: &dyn Configuration) {
        self.server = config.get_server_configs().cloned();
        self.peers = config.get_peer_configs().clone();
        self.spv_mode = config.is_spv_mode();
        self.browser_mode = config.is_browser();
        self.blockchain = config.get_blockchain_configs();
    }
}

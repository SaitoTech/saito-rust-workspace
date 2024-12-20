use std::io::{Error, ErrorKind};

use figment::providers::{Format, Json};
use figment::Figment;
use serde::Deserialize;

use log::{debug, error};
use saito_core::core::util::configuration::{
    BlockchainConfig, Configuration, ConsensusConfig, Endpoint, PeerConfig, Server,
};

#[derive(Deserialize, Debug, Clone)]
pub struct Spammer {
    pub timer_in_milli: u64,
    pub burst_count: u32,
    pub tx_size: u64,
    pub tx_count: u64,
    pub tx_payment: u64,
    pub tx_fee: u64,
    pub stop_after: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SpammerConfigs {
    server: Server,
    peers: Vec<PeerConfig>,
    spammer: Spammer,
    #[serde(skip)]
    lite: bool,
    consensus: Option<ConsensusConfig>,
}

impl SpammerConfigs {
    pub fn new() -> SpammerConfigs {
        SpammerConfigs {
            server: Server {
                host: "127.0.0.1".to_string(),
                port: 0,
                protocol: "http".to_string(),
                endpoint: Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 0,
                    protocol: "http".to_string(),
                },
                verification_threads: 4,
                channel_size: 0,
                stat_timer_in_ms: 0,
                reconnection_wait_time: 10000,
                thread_sleep_time_in_ms: 10,
                block_fetch_batch_size: 0,
            },
            peers: vec![],
            spammer: Spammer {
                timer_in_milli: 0,
                burst_count: 0,
                tx_size: 0,
                tx_count: 0,
                tx_payment: 0,
                tx_fee: 0,
                stop_after: 0,
            },
            lite: false,
            consensus: None,
        }
    }

    pub fn get_spammer_configs(&self) -> &Spammer {
        return &self.spammer;
    }
}

impl Configuration for SpammerConfigs {
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
            + "/block/"
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

    fn get_consensus_config(&self) -> Option<&ConsensusConfig> {
        self.consensus.as_ref()
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

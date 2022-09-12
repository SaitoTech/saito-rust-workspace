use saito_core::core::data::configuration::{Configuration, Endpoint, PeerConfig, Server};

pub struct WasmConfiguration {
    server: Server,
    peers: Vec<PeerConfig>,
}

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
            },
            peers: vec![],
        }
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
}

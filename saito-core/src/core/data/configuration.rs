use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Peer {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub synctype: String,
}

#[derive(Deserialize, Debug)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub protocol: String,
}

#[derive(Deserialize, Debug)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub endpoint: Endpoint,
}

#[derive(Deserialize, Debug)]
pub struct Configuration {
    pub server: Server,
    pub peers: Vec<Peer>,
}

impl Configuration {
    pub fn new() -> Configuration {
        Configuration {
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

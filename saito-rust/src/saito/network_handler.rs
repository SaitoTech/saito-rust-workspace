use std::collections::HashMap;

use log::{debug, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub struct NetworkHandler {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
    pub server: TcpListener,
}

impl NetworkHandler {}

pub async fn run_network_handler() {
    info!("running network handler");

    loop {}
}

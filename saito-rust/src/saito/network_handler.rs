use std::collections::HashMap;
use std::net::TcpStream;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::WebSocket;

pub struct NetworkHandler {
    sockets: HashMap<u64, WebSocket<MaybeTlsStream<TcpStream>>>,
}

impl NetworkHandler {}

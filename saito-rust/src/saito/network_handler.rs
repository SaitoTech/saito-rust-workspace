use std::collections::HashMap;
use std::net::SocketAddr;

use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, MaybeTlsStream, WebSocketStream};

use crate::Command;

pub struct NetworkHandler {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl NetworkHandler {}

pub async fn run_network_handler(sender: Sender<Command>) {
    info!("running network handler");

    let listener: TcpListener = TcpListener::bind("localhost:3000").await.unwrap();
    let network_handler = NetworkHandler {
        sockets: Default::default(),
    };
    tokio::spawn(async move {
        loop {
            let result: std::io::Result<(TcpStream, SocketAddr)> = listener.accept().await;
            if result.is_err() {
                error!("{:?}", result.err());
                continue;
            }
            let result = result.unwrap();
            let stream = result.0;
            let result = accept_async(stream).await.unwrap();
        }
    });
    loop {}
}

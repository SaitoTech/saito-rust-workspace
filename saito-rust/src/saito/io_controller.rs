use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, MaybeTlsStream, WebSocketStream};

use crate::Command;

pub struct IoController {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl IoController {
    pub fn process_message(&self, network_message: Command) {}
    pub fn process_file_request(&self, file_request: Command) {}
}

pub async fn run_io_controller(
    receiver: Receiver<Command>,
    sender_to_saito_controller: Sender<Command>,
) {
    info!("running network handler");

    let listener: TcpListener = TcpListener::bind("localhost:3000").await.unwrap();
    let network_handler = IoController {
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
    let mut work_done = false;

    loop {
        if !work_done {
            std::thread::sleep(Duration::new(0, 1000));
        }
    }
}

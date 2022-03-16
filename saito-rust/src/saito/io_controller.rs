use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, MaybeTlsStream, WebSocketStream};

use saito_core::core::peer::Peer;

use crate::Command;

pub struct IoController {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl IoController {
    pub fn process_network_message(&self, peer_index: u64, buffer: Vec<u8>) {
        todo!()
    }
    pub fn process_file_request(&self, file_request: Command) {}
}

struct PeerCounter {
    counter: u64,
}

impl PeerCounter {
    pub fn get_next_index(&mut self) -> u64 {
        self.counter = self.counter + 1;
        self.counter
    }
}

pub async fn run_io_controller(
    mut receiver: Receiver<Command>,
    sender_to_saito_controller: Sender<Command>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let listener: TcpListener = TcpListener::bind("localhost:3000").await.unwrap();
    let network_handler = IoController {
        sockets: Default::default(),
    };

    let io_controller = IoController {
        sockets: Default::default(),
    };

    let peer_counter_clone = peer_index_counter.clone();
    tokio::spawn(async move {
        info!("starting server");
        loop {
            let result: std::io::Result<(TcpStream, SocketAddr)> = listener.accept().await;
            if result.is_err() {
                error!("{:?}", result.err());
                continue;
            }
            info!("receiving incoming connection");
            let result = result.unwrap();
            let stream = result.0;
            let result = accept_async(stream).await.unwrap();

            let mut counter = peer_counter_clone.lock().unwrap();
            let peer = Peer::new(counter.get_next_index());

            // todo : add to peer list

            // todo : send PeerConnected command to saito lib
        }
    })
    .await;
    let mut work_done = false;

    loop {
        // let command = Command::NetworkMessage(10, [1, 2, 3].to_vec());
        //
        // sender_to_saito_controller.send(command).await;
        // info!("sending test message to saito controller");

        let result = receiver.try_recv();
        if result.is_ok() {
            let command = result.unwrap();
            work_done = true;
            match command {
                Command::OutgoingNetworkMessage(index, buffer) => {
                    io_controller.process_network_message(index, buffer);
                }
                Command::DataSaveRequest(_, _) => {}
                Command::DataSaveResponse(_, _) => {}
                Command::DataReadRequest(_) => {}
                Command::DataReadResponse(_, _, _) => {}
                Command::ConnectToPeer(_) => {}
                Command::PeerConnected(_, _) => {}
                Command::PeerDisconnected(_) => {}
                _ => {}
            }
        }

        if !work_done {
            std::thread::sleep(Duration::new(1, 0));
        }
    }
}

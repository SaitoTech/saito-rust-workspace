use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, MaybeTlsStream, WebSocketStream};

use saito_core::core::data::peer::Peer;

use crate::{InterfaceEvent, IoEvent};

pub struct IoController {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl IoController {
    pub fn process_network_message(&self, peer_index: u64, buffer: Vec<u8>) {
        todo!()
    }
    pub fn process_file_request(&self, file_request: InterfaceEvent) {}
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
    mut receiver: Receiver<IoEvent>,
    sender_to_saito_controller: Sender<IoEvent>,
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
            let peer = Peer::new(counter.get_next_index(), [0; 33]);

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
            let event = result.unwrap();
            let event = event.event;
            work_done = true;
            match event {
                InterfaceEvent::OutgoingNetworkMessage(index, buffer) => {
                    io_controller.process_network_message(index, buffer);
                }
                InterfaceEvent::DataSaveRequest(_, _) => {}
                InterfaceEvent::DataSaveResponse(_, _) => {}
                InterfaceEvent::DataReadRequest(_) => {}
                InterfaceEvent::DataReadResponse(_, _, _) => {}
                InterfaceEvent::ConnectToPeer(_) => {}
                InterfaceEvent::PeerConnected(_, _) => {}
                InterfaceEvent::PeerDisconnected(_) => {}
                _ => {}
            }
        }

        if !work_done {
            std::thread::sleep(Duration::new(1, 0));
        }
    }
}

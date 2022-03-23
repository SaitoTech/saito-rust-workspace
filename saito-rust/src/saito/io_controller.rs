use std::collections::HashMap;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, MaybeTlsStream, WebSocketStream};

use saito_core::core::data::block::BlockType;
use saito_core::core::data::peer::Peer;

use crate::{InterfaceEvent, IoEvent};

pub struct IoController {
    sockets: HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>,
    pub sender_to_saito_controller: Sender<IoEvent>,
}

impl IoController {
    pub fn send_outgoing_message(&self, peer_index: u64, buffer: Vec<u8>) {
        debug!("sending outgoing message : peer = {:?}", peer_index);
        let socket = self.sockets.get(&peer_index);
        if socket.is_none() {
            // TODO : handle
        }
        let socket = socket.unwrap();
    }
    pub fn process_file_request(&self, file_request: InterfaceEvent) {}
    async fn write_to_file(
        &self,
        request_key: String,
        filename: String,
        buffer: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        debug!(
            "writing to file : {:?} in {:?}",
            filename,
            std::env::current_dir().unwrap().to_str()
        );

        // TODO : run in a new thread with perf testing

        if !Path::new(&filename).exists() {
            let mut file = File::create(filename.clone()).unwrap();
            let result = file.write_all(&buffer[..]);
            if result.is_err() {
                warn!("failed writing file : {:?}", filename);
                let error = result.err().unwrap();
                warn!("{:?}", error);
                self.sender_to_saito_controller.send(IoEvent {
                    controller_id: 1,
                    event: InterfaceEvent::DataSaveResponse(request_key, Err(error)),
                });
                return Err(std::io::Error::from(ErrorKind::Other));
            }
        }
        debug!("file written successfully : {:?}", filename);
        self.sender_to_saito_controller
            .send(IoEvent {
                controller_id: 1,
                event: InterfaceEvent::DataSaveResponse(request_key, Ok(filename)),
            })
            .await;
        Ok(())
    }
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

// TODO : refactor to use ProcessEvent trait
pub async fn run_io_controller(
    mut receiver: Receiver<IoEvent>,
    sender_to_saito_controller: Sender<IoEvent>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let listener: TcpListener = TcpListener::bind("localhost:3000").await.unwrap();

    let io_controller = IoController {
        sockets: Default::default(),
        sender_to_saito_controller,
    };

    let peer_counter_clone = peer_index_counter.clone();
    let server_handle = tokio::spawn(async move {
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
    });
    let mut work_done = false;
    let controller_handle = tokio::spawn(async move {
        loop {
            // let command = Command::NetworkMessage(10, [1, 2, 3].to_vec());
            //
            // sender_to_saito_controller.send(command).await;
            // info!("sending test message to saito controller");

            let result = receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                let interface_event = event.event;
                work_done = true;
                match interface_event {
                    InterfaceEvent::OutgoingNetworkMessage(index, message_name, buffer) => {
                        io_controller.send_outgoing_message(index, buffer);
                    }
                    InterfaceEvent::DataSaveRequest(index, key, buffer) => {
                        io_controller.write_to_file(index, key, buffer).await;
                    }
                    InterfaceEvent::DataSaveResponse(_, _) => {
                        unreachable!()
                    }
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
    });
    tokio::join!(server_handle, controller_handle);
}

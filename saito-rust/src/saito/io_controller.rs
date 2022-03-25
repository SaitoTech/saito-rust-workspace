use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::{connect, Message};

use saito_core::common::command::InterfaceEvent::PeerConnectionResult;
use saito_core::core::data;
use saito_core::core::data::block::BlockType;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::peer::Peer;

use crate::{InterfaceEvent, IoEvent};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct IoController {
    sockets: HashMap<u64, SocketSender>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    pub sender_to_saito_controller: Sender<IoEvent>,
}

impl IoController {
    pub async fn send_outgoing_message(&self, peer_index: u64, buffer: Vec<u8>) {
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
                    event: InterfaceEvent::DataSaveResponse {
                        key: request_key,
                        result: Err(error),
                    },
                });
                return Err(std::io::Error::from(ErrorKind::Other));
            }
        }
        debug!("file written successfully : {:?}", filename);
        self.sender_to_saito_controller
            .send(IoEvent {
                controller_id: 1,
                event: InterfaceEvent::DataSaveResponse {
                    key: request_key,
                    result: Ok(filename),
                },
            })
            .await;
        Ok(())
    }
    pub async fn connect_to_peer(
        io_controller: Arc<RwLock<IoController>>,
        peer: data::configuration::Peer,
    ) {
        // TODO : handle connecting to an already connected (via incoming connection) node.

        debug!("connecting to peer : {:?}", peer);
        let mut protocol: String = String::from("ws");
        if peer.protocol == "https" {
            protocol = String::from("wss");
        }
        let url = protocol + "://" + peer.host.as_str() + ":" + peer.port.to_string().as_str();
        let result = connect_async(url).await;
        if result.is_err() {
            warn!("failed connecting to peer : {:?}", peer);
            let io_controller = io_controller.write().await;
            io_controller
                .sender_to_saito_controller
                .send(IoEvent {
                    controller_id: 1,
                    event: PeerConnectionResult {
                        peer_details: Some(peer),
                        result: Err(Error::from(ErrorKind::Other)),
                    },
                })
                .await;
            return;
        }
        let result = result.unwrap();
        let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;

        let mut io_controller = io_controller.write().await;
        let sender_to_controller = io_controller.sender_to_saito_controller.clone();
        IoController::send_new_peer(
            io_controller.peer_counter.clone(),
            &mut io_controller.sockets,
            socket,
            sender_to_controller,
        )
        .await;
    }
    pub async fn send_to_all(
        &mut self,
        message_name: String,
        buffer: Vec<u8>,
        exceptions: Vec<u64>,
    ) {
        debug!("sending message : {:?} to all", message_name);
        for entry in self.sockets.iter_mut() {
            if exceptions.contains(&entry.0) {
                continue;
            }
            let mut socket = entry.1;
            socket.send(Message::Binary(buffer.clone())).await;
        }
        debug!("message {:?} sent to all", message_name);
    }
    pub async fn send_new_peer(
        peer_counter: Arc<Mutex<PeerCounter>>,
        sockets: &mut HashMap<u64, SocketSender>,
        socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
        sender: Sender<IoEvent>,
    ) {
        let mut counter = peer_counter.lock().await;
        let next_index = counter.get_next_index();

        let (socket_sender, mut socket_receiver): (SocketSender, SocketReceiver) = socket.split();

        sockets.insert(next_index, socket_sender);
        debug!("sending new peer : {:?}", next_index);
        sender
            .send(IoEvent {
                controller_id: 1,
                event: PeerConnectionResult {
                    peer_details: None,
                    result: Ok(next_index),
                },
            })
            .await;

        IoController::receive_message_from_peer(socket_receiver, sender.clone(), next_index);
        debug!("new peer : {:?} processed successfully", next_index);
    }
    pub async fn receive_message_from_peer(
        mut receiver: SocketReceiver,
        sender: Sender<IoEvent>,
        next_index: u64,
    ) -> JoinHandle<()> {
        debug!("starting new task for reading from peer : {:?}", next_index);
        tokio::spawn(async move {
            debug!("new thread started for peer receiving");
            loop {
                let result = receiver.next().await;
                if result.is_none() {
                    continue;
                }
                let result = result.unwrap();
                if result.is_err() {
                    warn!("{:?}", result.err().unwrap());
                    continue;
                }
                let result = result.unwrap();
                match result {
                    Message::Binary(buffer) => {
                        let message = IoEvent {
                            controller_id: 1,
                            event: InterfaceEvent::IncomingNetworkMessage {
                                peer_index: next_index,
                                message_name: "TEST".to_string(),
                                buffer,
                            },
                        };
                        sender.send(message).await;
                    }
                    _ => {
                        // Not handling these scenarios
                    }
                }
            }
        })
    }
}

pub struct PeerCounter {
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
    configs: Arc<RwLock<Configuration>>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let mut url = "".to_string();
    {
        let configs = configs.write().await;
        url = "localhost:".to_string() + configs.server.port.to_string().as_str();
    }

    info!("starting server on : {:?}", url);
    let listener: TcpListener = TcpListener::bind(url).await.unwrap();
    let peer_counter_clone = peer_index_counter.clone();
    let sender_clone = sender_to_saito_controller.clone();

    let io_controller = Arc::new(RwLock::new(IoController {
        sockets: Default::default(),
        sender_to_saito_controller,
        peer_counter: peer_index_counter.clone(),
    }));

    let io_controller_clone = io_controller.clone();
    let server_handle = run_server(
        listener,
        peer_counter_clone,
        sender_clone,
        io_controller_clone,
    );
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
                    InterfaceEvent::OutgoingNetworkMessageForAll {
                        message_name,
                        buffer,
                        exceptions,
                    } => {
                        let mut io_controller = io_controller.write().await;
                        io_controller
                            .send_to_all(message_name, buffer, exceptions)
                            .await;
                    }
                    InterfaceEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        message_name: message_name,
                        buffer: buffer,
                    } => {
                        let io_controller = io_controller.write().await;
                        io_controller.send_outgoing_message(index, buffer);
                    }
                    InterfaceEvent::DataSaveRequest {
                        key: index,
                        filename: key,
                        buffer: buffer,
                    } => {
                        let io_controller = io_controller.write().await;
                        io_controller.write_to_file(index, key, buffer).await;
                    }
                    InterfaceEvent::DataSaveResponse { key: _, result: _ } => {
                        unreachable!()
                    }
                    InterfaceEvent::DataReadRequest(_) => {}
                    InterfaceEvent::DataReadResponse(_, _, _) => {}
                    InterfaceEvent::ConnectToPeer { peer_details } => {
                        IoController::connect_to_peer(io_controller.clone(), peer_details).await;
                    }
                    InterfaceEvent::PeerConnectionResult {
                        peer_details,
                        result,
                    } => {
                        unreachable!()
                    }
                    InterfaceEvent::PeerDisconnected { peer_index } => {}
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

fn run_server(
    listener: TcpListener,
    peer_counter_clone: Arc<Mutex<PeerCounter>>,
    sender_clone: Sender<IoEvent>,
    io_controller_clone: Arc<RwLock<IoController>>,
) -> JoinHandle<()> {
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
            let stream = MaybeTlsStream::Plain(result.0);
            let stream = accept_async(stream).await.unwrap();
            let mut controller = io_controller_clone.write().await;
            IoController::send_new_peer(
                peer_counter_clone.clone(),
                &mut controller.sockets,
                stream,
                sender_clone.clone(),
            )
            .await;
            // let mut counter = peer_counter_clone.lock().unwrap();
            // let next_index = counter.get_next_index();
            //
            // let controller = io_controller_clone.write().await;
            // controller.sockets.insert(next_index, stream);
            //
            // sender_clone.send(IoEvent {
            //     controller_id: 1,
            //     event: PeerConnectionResult {
            //         peer_details: None,
            //         result: Ok(next_index),
            //     },
            // });
        }
    })
}

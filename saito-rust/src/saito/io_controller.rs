use std::collections::HashMap;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, info, trace, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::common::command::InterfaceEvent::PeerConnectionResult;
use saito_core::common::defs::SaitoHash;
use saito_core::core::data;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;

use crate::saito::rust_io_handler::{FutureState, RustIOHandler};
use crate::{InterfaceEvent, IoEvent};

type SocketSender =
    SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct IoController {
    sockets: HashMap<u64, PeerSender>,
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
        event_id: u64,
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
                // self.sender_to_saito_controller.send(IoEvent::new(
                //     InterfaceEvent::DataSaveResponse {
                //         key: request_key,
                //         result: Err(error),
                //     },
                // ));
                RustIOHandler::set_event_response(event_id, FutureState::DataSaved(Err(error)));
                return Err(std::io::Error::from(ErrorKind::Other));
            }
        }
        debug!("file written successfully : {:?}", filename);
        RustIOHandler::set_event_response(event_id, FutureState::DataSaved(Ok(filename)));
        // self.sender_to_saito_controller
        //     .send(IoEvent::new(InterfaceEvent::DataSaveResponse {
        //         key: request_key,
        //         result: Ok(filename),
        //     }))
        //     .await;
        Ok(())
    }
    pub async fn connect_to_peer(
        event_id: u64,
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
            RustIOHandler::set_event_response(
                event_id,
                FutureState::PeerConnectionResult(Err(Error::from(ErrorKind::Other))),
            );
            return;
        }
        let result = result.unwrap();
        let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;

        trace!("waiting for the io controller write lock");
        let mut io_controller = io_controller.write().await;
        trace!("acquired the io controller write lock");
        let sender_to_controller = io_controller.sender_to_saito_controller.clone();
        let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) = socket.split();
        IoController::send_new_peer(
            event_id,
            io_controller.peer_counter.clone(),
            &mut io_controller.sockets,
            PeerSender::Tungstenite(socket_sender),
            PeerReceiver::Tungstenite(socket_receiver),
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
            let socket = entry.1;
            match socket {
                PeerSender::Warp(sender) => {
                    sender
                        .send(warp::ws::Message::binary(buffer.clone()))
                        .await
                        .unwrap();
                }
                PeerSender::Tungstenite(sender) => {
                    sender
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            buffer.clone(),
                        ))
                        .await
                        .unwrap();
                }
            }
        }
        debug!("message {:?} sent to all", message_name);
    }
    pub async fn fetch_block(url: String, event_id: u64) {
        debug!("fetching block : {:?}", url);

        let result = reqwest::get(url).await;
        if result.is_err() {
            todo!()
        }
        let response = result.unwrap();
        let result = response.bytes().await;
        if result.is_err() {
            todo!()
        }
        let result = result.unwrap();
        let buffer = result.to_vec();
        let block = Block::deserialize_for_net(&buffer);
        RustIOHandler::set_event_response(event_id, FutureState::BlockFetched(block));
    }
    pub async fn send_new_peer(
        event_id: u64,
        peer_counter: Arc<Mutex<PeerCounter>>,
        sockets: &mut HashMap<u64, PeerSender>,
        sender: PeerSender,
        receiver: PeerReceiver,
        sender_to_core: Sender<IoEvent>,
    ) {
        let mut counter = peer_counter.lock().await;
        let next_index = counter.get_next_index();

        sockets.insert(next_index, sender);
        debug!("sending new peer : {:?}", next_index);
        if event_id != 0 {
            RustIOHandler::set_event_response(
                event_id,
                FutureState::PeerConnectionResult(Ok(next_index)),
            );
        } else {
            sender_to_core
                .send(IoEvent {
                    controller_id: 1,
                    event_id,
                    event: PeerConnectionResult {
                        peer_details: None,
                        result: Ok(next_index),
                    },
                })
                .await
                .expect("sending failed");
        }

        IoController::receive_message_from_peer(receiver, sender_to_core.clone(), next_index).await;
        debug!("new peer : {:?} processed successfully", next_index);
    }
    pub async fn receive_message_from_peer(
        mut receiver: PeerReceiver,
        sender: Sender<IoEvent>,
        next_index: u64,
    ) {
        debug!("starting new task for reading from peer : {:?}", next_index);
        tokio::spawn(async move {
            debug!("new thread started for peer receiving");
            match receiver {
                PeerReceiver::Warp(mut receiver) => loop {
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
                    if result.is_binary() {
                        let buffer = result.into_bytes();
                        let message = IoEvent {
                            controller_id: 1,
                            event_id: 0,
                            event: InterfaceEvent::IncomingNetworkMessage {
                                peer_index: next_index,
                                message_name: "TEST".to_string(),
                                buffer,
                            },
                        };
                        sender.send(message).await.expect("sending failed");
                    }
                },
                PeerReceiver::Tungstenite(mut receiver) => loop {
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
                        tokio_tungstenite::tungstenite::Message::Binary(buffer) => {
                            let message = IoEvent {
                                controller_id: 1,
                                event_id: 0,
                                event: InterfaceEvent::IncomingNetworkMessage {
                                    peer_index: next_index,
                                    message_name: "TEST".to_string(),
                                    buffer,
                                },
                            };
                            sender.send(message).await.expect("sending failed");
                        }
                        _ => {
                            // Not handling these scenarios
                        }
                    }
                },
            }
        });
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
    blockchain: Arc<RwLock<Blockchain>>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let url;
    let port;
    {
        trace!("waiting for the configs write lock");
        let configs = configs.read().await;
        trace!("acquired the configs write lock");
        url = "localhost:".to_string() + configs.server.port.to_string().as_str();
        port = configs.server.port;
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

    let server_handle = run_websocket_server(
        peer_counter_clone,
        sender_clone.clone(),
        io_controller_clone.clone(),
        port,
    );
    const WEB_SERVER_PORT_OFFSET: u16 = 1;
    let web_server_handle = run_web_server(
        sender_clone.clone(),
        io_controller_clone.clone(),
        port + WEB_SERVER_PORT_OFFSET,
        blockchain,
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
                let event_id = event.event_id;
                let interface_event = event.event;
                work_done = true;
                match interface_event {
                    InterfaceEvent::OutgoingNetworkMessageForAll {
                        message_name,
                        buffer,
                        exceptions,
                    } => {
                        trace!("waiting for the io controller write lock");
                        let mut io_controller = io_controller.write().await;
                        trace!("acquired the io controller write lock");
                        io_controller
                            .send_to_all(message_name, buffer, exceptions)
                            .await;
                    }
                    InterfaceEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        message_name,
                        buffer,
                    } => {
                        trace!("waiting for the io_controller write lock");
                        let io_controller = io_controller.read().await;
                        trace!("acquired the io controller write lock");
                        io_controller.send_outgoing_message(index, buffer).await;
                    }
                    InterfaceEvent::ConnectToPeer { peer_details } => {
                        IoController::connect_to_peer(
                            event_id,
                            io_controller.clone(),
                            peer_details,
                        )
                        .await;
                    }
                    InterfaceEvent::PeerConnectionResult { .. } => {
                        unreachable!()
                    }
                    InterfaceEvent::PeerDisconnected { peer_index } => {}
                    InterfaceEvent::IncomingNetworkMessage { .. } => {}
                    InterfaceEvent::BlockFetchRequest { url } => {
                        IoController::fetch_block(url, event_id).await
                    }
                }
            }

            if !work_done {
                std::thread::sleep(Duration::new(1, 0));
            }
        }
    });
    let result = tokio::join!(server_handle, controller_handle, web_server_handle);
}

pub enum PeerSender {
    Warp(SplitSink<WebSocket, warp::ws::Message>),
    Tungstenite(SocketSender),
}

pub enum PeerReceiver {
    Warp(SplitStream<WebSocket>),
    Tungstenite(SocketReceiver),
}

fn run_websocket_server(
    peer_counter: Arc<Mutex<PeerCounter>>,
    sender_clone: Sender<IoEvent>,
    io_controller: Arc<RwLock<IoController>>,
    port: u16,
) -> JoinHandle<()> {
    info!("running websocket server on {:?}", port);
    tokio::spawn(async move {
        info!("starting server");
        let io_controller = io_controller.clone();
        let sender_to_io = sender_clone.clone();
        let peer_counter = peer_counter.clone();
        let ws_route = warp::path("ws")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                let clone = io_controller.clone();
                let peer_counter = peer_counter.clone();
                let sender_to_io = sender_to_io.clone();
                ws.on_upgrade(move |socket| async move {
                    let (sender, receiver) = socket.split();
                    let mut controller = clone.write().await;

                    IoController::send_new_peer(
                        0,
                        peer_counter,
                        &mut controller.sockets,
                        PeerSender::Warp(sender),
                        PeerReceiver::Warp(receiver),
                        sender_to_io,
                    )
                    .await
                })
            });

        let (_, server) =
            warp::serve(ws_route).bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
                // tokio::signal::ctrl_c().await.ok();
            });
        server.await;
    })
}

fn run_web_server(
    sender_clone: Sender<IoEvent>,
    io_controller_clone: Arc<RwLock<IoController>>,
    port: u16,
    blockchain: Arc<RwLock<Blockchain>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        debug!("running web server on : {:?}", port);
        let blockchain = blockchain.clone();
        // TODO : handle security aspect of the server connections
        let http_route = warp::path!("block" / String)
            .and(warp::any().map(move || blockchain.clone()))
            .and_then(
                |block_hash: String, blockchain: Arc<RwLock<Blockchain>>| async move {
                    debug!("serving block : {:?}", block_hash);
                    let buffer: Vec<u8>;
                    {
                        let block_hash = hex::decode(block_hash);
                        if block_hash.is_err() {
                            todo!()
                        }
                        let block_hash = block_hash.unwrap();
                        if block_hash.len() != 32 {
                            todo!()
                        }
                        let block_hash: SaitoHash = block_hash.try_into().unwrap();
                        // TODO : load disk from disk and serve rather than locking the blockchain
                        let blockchain = blockchain.read().await;
                        let block = blockchain.get_block(&block_hash).await;
                        if block.is_none() {
                            debug!("block not found : {:?}", block_hash);
                            buffer = vec![];
                            return Err(warp::reject::not_found());
                        }
                        // TODO : check if the full block is in memory or need to load from disk
                        buffer = block.unwrap().serialize_for_net(BlockType::Full);
                    }
                    Ok(warp::reply::with_status(buffer, StatusCode::OK))
                },
            );
        info!("starting server");
        let (_, server) =
            warp::serve(http_route).bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
                // tokio::signal::ctrl_c().await.ok();
            });
        server.await;
        // warp::serve(http_route).run(([127, 0, 0, 1], port)).await;
    })
}

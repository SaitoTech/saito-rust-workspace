use std::collections::HashMap;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::common::defs::SaitoHash;
use saito_core::core::data;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, Peer};

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
    pub async fn send_outgoing_message(&mut self, peer_index: u64, buffer: Vec<u8>) {
        debug!("sending outgoing message : peer = {:?}", peer_index);
        let socket = self.sockets.get_mut(&peer_index);
        if socket.is_none() {
            todo!()
        }
        let socket = socket.unwrap();
        match socket {
            PeerSender::Warp(sender) => {
                sender
                    .send(warp::ws::Message::binary(buffer.clone()))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
            PeerSender::Tungstenite(sender) => {
                sender
                    .send(tokio_tungstenite::tungstenite::Message::Binary(
                        buffer.clone(),
                    ))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
        }
    }

    pub async fn connect_to_peer(
        event_id: u64,
        io_controller: Arc<RwLock<IoController>>,
        peer: data::configuration::Peer,
    ) {
        // TODO : handle connecting to an already connected (via incoming connection) node.

        let mut protocol: String = String::from("ws");
        if peer.protocol == "https" {
            protocol = String::from("wss");
        }
        let url = protocol
            + "://"
            + peer.host.as_str()
            + ":"
            + peer.port.to_string().as_str()
            + "/wsopen";
        debug!("connecting to peer : {:?}", url);
        let result = connect_async(url.clone()).await;
        if result.is_err() {
            warn!("failed connecting to peer : {:?}", peer);
            error!("{:?}", result.err());
            RustIOHandler::set_event_response(
                event_id,
                FutureState::PeerConnectionResult(Err(Error::from(ErrorKind::Other))),
            );
            return;
        }
        debug!("connected to peer : {:?}", url);
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
            Some(peer),
        )
        .await;
    }
    pub async fn send_to_all(&mut self, buffer: Vec<u8>, exceptions: Vec<u64>) {
        debug!("sending message : {:?} to all", buffer[0]);
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
        trace!("message sent to all");
    }
    pub async fn fetch_block(
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        event_id: u64,
        sender_to_core: Sender<IoEvent>,
    ) {
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
        debug!("block buffer received");
        // RustIOHandler::set_event_response(event_id, FutureState::BlockFetched(block));
        sender_to_core
            .send(IoEvent {
                controller_id: 1,
                event_id,
                event: InterfaceEvent::BlockFetched {
                    block_hash,
                    peer_index,
                    buffer,
                },
            })
            .await
            .unwrap();
    }
    pub async fn send_new_peer(
        event_id: u64,
        peer_counter: Arc<Mutex<PeerCounter>>,
        sockets: &mut HashMap<u64, PeerSender>,
        sender: PeerSender,
        receiver: PeerReceiver,
        sender_to_core: Sender<IoEvent>,
        peer_data: Option<Peer>,
    ) {
        let mut counter = peer_counter.lock().await;
        let next_index = counter.get_next_index();

        sockets.insert(next_index, sender);
        debug!("sending new peer : {:?}", next_index);

        sender_to_core
            .send(IoEvent {
                controller_id: 1,
                event_id,
                event: InterfaceEvent::PeerConnectionResult {
                    peer_details: peer_data,
                    result: Ok(next_index),
                },
            })
            .await
            .expect("sending failed");

        IoController::receive_message_from_peer(receiver, sender_to_core.clone(), next_index).await;
    }
    pub async fn receive_message_from_peer(
        mut receiver: PeerReceiver,
        sender: Sender<IoEvent>,
        peer_index: u64,
    ) {
        debug!("starting new task for reading from peer : {:?}", peer_index);
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
                        // TODO : handle peer disconnections
                        warn!("failed receiving message [1] : {:?}", result.err().unwrap());
                        break;
                    }
                    let result = result.unwrap();

                    if result.is_binary() {
                        let buffer = result.into_bytes();
                        let message = IoEvent {
                            controller_id: 1,
                            event_id: 0,
                            event: InterfaceEvent::IncomingNetworkMessage { peer_index, buffer },
                        };
                        sender.send(message).await.expect("sending failed");
                    } else {
                        todo!()
                    }
                },
                PeerReceiver::Tungstenite(mut receiver) => loop {
                    let result = receiver.next().await;
                    if result.is_none() {
                        continue;
                    }
                    let result = result.unwrap();
                    if result.is_err() {
                        warn!("failed receiving message [2] : {:?}", result.err().unwrap());
                        break;
                    }
                    let result = result.unwrap();
                    match result {
                        tokio_tungstenite::tungstenite::Message::Binary(buffer) => {
                            let message = IoEvent {
                                controller_id: 1,
                                event_id: 0,
                                event: InterfaceEvent::IncomingNetworkMessage {
                                    peer_index,
                                    buffer,
                                },
                            };
                            sender.send(message).await.expect("sending failed");
                        }
                        _ => {
                            // Not handling these scenarios
                            todo!()
                        }
                    }
                },
            }
        });
        // TODO : check why the thread is not starting without this line. probably the parent thread is blocked from somewhere.
        // tokio::task::yield_now().await;
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
        blockchain.clone(),
    );

    let mut work_done = false;
    let controller_handle = tokio::spawn(async move {
        loop {
            // let command = Command::NetworkMessage(10, [1, 2, 3].to_vec());
            //
            // sender_to_saito_controller.send(command).await;
            // info!("sending test message to saito controller");

            let result = receiver.recv().await;
            if result.is_some() {
                let event = result.unwrap();
                let event_id = event.event_id;
                let interface_event = event.event;
                work_done = true;
                match interface_event {
                    InterfaceEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        trace!("waiting for the io controller write lock");
                        let mut io_controller = io_controller.write().await;
                        trace!("acquired the io controller write lock");
                        io_controller.send_to_all(buffer, exceptions).await;
                    }
                    InterfaceEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        buffer,
                    } => {
                        trace!("waiting for the io_controller write lock");
                        let mut io_controller = io_controller.write().await;
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
                    InterfaceEvent::BlockFetchRequest {
                        block_hash,
                        peer_index,
                        url,
                    } => {
                        let sender;
                        {
                            let io_controller = io_controller.read().await;
                            sender = io_controller.sender_to_saito_controller.clone();
                        }
                        tokio::spawn(async move {
                            IoController::fetch_block(block_hash, peer_index, url, event_id, sender)
                                .await
                        });
                    }
                    InterfaceEvent::BlockFetched { .. } => {
                        unreachable!()
                    }
                }
            }

            // if !work_done {
            //     std::thread::sleep(Duration::new(1, 0));
            // }
        }
    });
    let result = tokio::join!(server_handle, controller_handle);
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
    blockchain: Arc<RwLock<Blockchain>>,
) -> JoinHandle<()> {
    info!("running websocket server on {:?}", port);
    tokio::spawn(async move {
        info!("starting websocket server");
        let io_controller = io_controller.clone();
        let sender_to_io = sender_clone.clone();
        let peer_counter = peer_counter.clone();
        let ws_route = warp::path("wsopen")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                debug!("incoming connection received");
                let clone = io_controller.clone();
                let peer_counter = peer_counter.clone();
                let sender_to_io = sender_to_io.clone();
                ws.on_upgrade(move |socket| async move {
                    debug!("socket connection established");
                    let (sender, receiver) = socket.split();
                    let mut controller = clone.write().await;

                    IoController::send_new_peer(
                        0,
                        peer_counter,
                        &mut controller.sockets,
                        PeerSender::Warp(sender),
                        PeerReceiver::Warp(receiver),
                        sender_to_io,
                        None,
                    )
                    .await
                })
            });
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
                        trace!("waiting for the blockchain write lock");
                        // TODO : load disk from disk and serve rather than locking the blockchain
                        let blockchain = blockchain.read().await;
                        trace!("acquired the blockchain write lock");
                        let block = blockchain.get_block(&block_hash).await;
                        if block.is_none() {
                            debug!("block not found : {:?}", block_hash);
                            return Err(warp::reject::not_found());
                        }
                        // TODO : check if the full block is in memory or need to load from disk
                        buffer = block.unwrap().serialize_for_net(BlockType::Full);
                    }
                    Ok(warp::reply::with_status(buffer, StatusCode::OK))
                },
            );
        let routes = http_route.or(ws_route);
        // let (_, server) =
        //     warp::serve(ws_route).bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
        //         // tokio::signal::ctrl_c().await.ok();
        //     });
        // server.await;
        warp::serve(routes).run(([127, 0, 0, 1], port)).await;
    })
}

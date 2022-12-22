use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, trace, warn};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::common::defs::{
    push_lock, SaitoHash, StatVariable, BLOCK_FILE_EXTENSION, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_NETWORK_CONTROLLER, STAT_BIN_COUNT,
};
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::lock_for_read;

use crate::saito::rust_io_handler::BLOCKS_DIR_PATH;
use crate::{IoEvent, NetworkEvent, TimeKeeper};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct NetworkController {
    sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    currently_queried_urls: Arc<Mutex<HashSet<String>>>,
    pub sender_to_saito_controller: Sender<IoEvent>,
}

impl NetworkController {
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn send(connection: &mut PeerSender, peer_index: u64, buffer: Vec<u8>) -> bool {
        let mut send_failed = false;

        match connection {
            PeerSender::Warp(sender) => {
                if let Err(error) = sender.send(warp::ws::Message::binary(buffer)).await {
                    error!(
                        "Error sending message, Peer Index = {:?}, Reason {:?}",
                        peer_index, error
                    );

                    send_failed = true;
                }
                // if let Err(error) = sender.flush().await {
                //     error!(
                //         "Error flushing connection, Peer Index = {:?}, Reason {:?}",
                //         peer_index, error
                //     );
                //     send_failed = true;
                // }
            }
            PeerSender::Tungstenite(sender) => {
                if let Err(error) = sender
                    .send(tokio_tungstenite::tungstenite::Message::Binary(buffer))
                    .await
                {
                    error!(
                        "Error sending message, Peer Index = {:?}, Reason {:?}",
                        peer_index, error
                    );
                    send_failed = true;
                }
                // if let Err(error) = sender.flush().await {
                //     error!(
                //         "Error flushing connection, Peer Index = {:?}, Reason {:?}",
                //         peer_index, error
                //     );
                //     send_failed = true;
                // }
            }
        }

        return !send_failed;
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn send_outgoing_message(
        sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
        peer_index: u64,
        buffer: Vec<u8>,
    ) {
        debug!("sending outgoing message : peer = {:?}", peer_index);
        let mut sockets = sockets.lock().await;
        let socket = sockets.get_mut(&peer_index);
        if socket.is_none() {
            error!(
                "Cannot find the corresponding sender socket, Peer Index : {:?}",
                peer_index
            );
            return;
        }

        let socket = socket.unwrap();

        if !Self::send(socket, peer_index, buffer).await {
            sockets.remove(&peer_index);
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn connect_to_peer(
        event_id: u64,
        io_controller: Arc<RwLock<NetworkController>>,
        peer: data::configuration::PeerConfig,
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
        if result.is_ok() {
            let result = result.unwrap();
            let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;

            let (network_controller, _network_controller_) =
                lock_for_read!(io_controller, LOCK_ORDER_NETWORK_CONTROLLER);

            let sender_to_controller = network_controller.sender_to_saito_controller.clone();
            let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) = socket.split();

            let peer_index;
            {
                let mut counter = network_controller.peer_counter.lock().await;
                peer_index = counter.get_next_index();
            }
            info!(
                "connected to peer : {:?} with index : {:?}",
                url, peer_index
            );

            NetworkController::send_new_peer(
                event_id,
                peer_index,
                network_controller.sockets.clone(),
                PeerSender::Tungstenite(socket_sender),
                PeerReceiver::Tungstenite(socket_receiver),
                sender_to_controller,
                Some(peer),
            )
            .await;
        } else {
            warn!(
                "failed connecting to : {:?}, reason {:?}",
                url,
                result.err().unwrap()
            );
        }
    }
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn send_to_all(
        sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
        buffer: Vec<u8>,
        exceptions: Vec<u64>,
    ) {
        trace!("sending message : {:?} to all", buffer[0]);
        let mut sockets = sockets.lock().await;
        let mut peers_with_errors: Vec<u64> = Default::default();

        for entry in sockets.iter_mut() {
            let peer_index = entry.0;
            if exceptions.contains(&peer_index) {
                continue;
            }
            let socket = entry.1;

            if !Self::send(socket, *peer_index, buffer.clone()).await {
                peers_with_errors.push(*peer_index)
            }
        }

        for peer in peers_with_errors {
            sockets.remove(&peer);
        }

        trace!("message sent to all");
    }
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn fetch_block(
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        event_id: u64,
        sender_to_core: Sender<IoEvent>,
        current_queries: Arc<Mutex<HashSet<String>>>,
    ) {
        debug!("fetching block : {:?}", url);

        {
            // since the block sizes can be large, we need to make sure same block is not fetched multiple times before first fetch finishes.
            let mut queries = current_queries.lock().await;
            if queries.contains(&url) {
                debug!("url : {:?} is already being fetched", url);
                return;
            }
            queries.insert(url.clone());
        }
        let result = reqwest::get(url.clone()).await;
        if result.is_err() {
            // TODO : should we retry here?
            warn!("failed fetching : {:?}", url);
            return;
        }
        let response = result.unwrap();
        let result = response.bytes().await;
        if result.is_err() {
            warn!("failed getting byte buffer from fetching block : {:?}", url);
            return;
        }
        let result = result.unwrap();
        let buffer = result.to_vec();

        debug!(
            "block buffer received with size : {:?} for url : {:?}",
            buffer.len(),
            url
        );
        // RustIOHandler::set_event_response(event_id, FutureState::BlockFetched(block));
        sender_to_core
            .send(IoEvent {
                event_processor_id: 1,
                event_id,
                event: NetworkEvent::BlockFetched {
                    block_hash,
                    peer_index,
                    buffer,
                },
            })
            .await
            .unwrap();
        {
            // since we have already fetched the block, we will remove it from the set.
            let mut queries = current_queries.lock().await;
            queries.remove(&url);
        }
        debug!("block buffer sent to blockchain controller");
    }
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn send_new_peer(
        event_id: u64,
        peer_index: u64,
        sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
        sender: PeerSender,
        receiver: PeerReceiver,
        sender_to_core: Sender<IoEvent>,
        peer_data: Option<PeerConfig>,
    ) {
        {
            sockets.lock().await.insert(peer_index, sender);
        }

        debug!("sending new peer : {:?}", peer_index);

        sender_to_core
            .send(IoEvent {
                event_processor_id: 1,
                event_id,
                event: NetworkEvent::PeerConnectionResult {
                    peer_details: peer_data,
                    result: Ok(peer_index),
                },
            })
            .await
            .expect("sending failed");

        NetworkController::receive_message_from_peer(
            receiver,
            sender_to_core.clone(),
            peer_index,
            sockets,
        )
        .await;
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn send_peer_disconnect(sender_to_core: Sender<IoEvent>, peer_index: u64) {
        debug!("sending peer disconnect : {:?}", peer_index);

        sender_to_core
            .send(IoEvent {
                event_processor_id: 1,
                event_id: 0,
                event: NetworkEvent::PeerDisconnected { peer_index },
            })
            .await
            .expect("sending failed");
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn receive_message_from_peer(
        receiver: PeerReceiver,
        sender: Sender<IoEvent>,
        peer_index: u64,
        sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
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
                        NetworkController::send_peer_disconnect(sender, peer_index).await;
                        sockets.lock().await.remove(&peer_index);
                        break;
                    }
                    let result = result.unwrap();

                    if result.is_binary() {
                        let buffer = result.into_bytes();
                        trace!(
                            "message buffer with size : {:?} received from peer : {:?}",
                            buffer.len(),
                            peer_index
                        );
                        let message = IoEvent {
                            event_processor_id: 1,
                            event_id: 0,
                            event: NetworkEvent::IncomingNetworkMessage { peer_index, buffer },
                        };
                        sender.send(message).await.expect("sending failed");
                    } else {
                        todo!("handle these scenarios 1")
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
                        NetworkController::send_peer_disconnect(sender, peer_index).await;
                        sockets.lock().await.remove(&peer_index);
                        break;
                    }
                    let result = result.unwrap();
                    match result {
                        tokio_tungstenite::tungstenite::Message::Binary(buffer) => {
                            trace!(
                                "message buffer with size : {:?} received from peer : {:?}",
                                buffer.len(),
                                peer_index
                            );
                            let message = IoEvent {
                                event_processor_id: 1,
                                event_id: 0,
                                event: NetworkEvent::IncomingNetworkMessage { peer_index, buffer },
                            };
                            sender.send(message).await.expect("sending failed");
                        }
                        _ => {
                            // Not handling these scenarios
                            todo!("handle these scenarios 2")
                        }
                    }
                },
            }
            debug!("listening thread existed for peer : {:?}", peer_index);
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
pub async fn run_network_controller(
    mut receiver: Receiver<IoEvent>,
    sender: Sender<IoEvent>,
    configs: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    blockchain: Arc<RwLock<Blockchain>>,
    sender_to_stat: Sender<String>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let host;
    let url;
    let port;
    {
        let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);

        url = configs.get_server_configs().host.clone()
            + ":"
            + configs.get_server_configs().port.to_string().as_str();
        port = configs.get_server_configs().port;
        host = configs.get_server_configs().host.clone();
    }

    info!("starting server on : {:?}", url);
    let peer_counter_clone = peer_index_counter.clone();
    let sender_clone = sender.clone();

    let network_controller = Arc::new(RwLock::new(NetworkController {
        sockets: Arc::new(Mutex::new(HashMap::new())),
        sender_to_saito_controller: sender,
        peer_counter: peer_index_counter.clone(),
        currently_queried_urls: Arc::new(Default::default()),
    }));

    let network_controller_clone = network_controller.clone();

    let server_handle = run_websocket_server(
        peer_counter_clone,
        sender_clone.clone(),
        network_controller_clone.clone(),
        port,
        host,
        blockchain.clone(),
    );

    let mut work_done = false;
    let controller_handle = tokio::spawn(async move {
        let mut outgoing_messages = StatVariable::new(
            "network::outgoing_msgs".to_string(),
            STAT_BIN_COUNT,
            sender_to_stat.clone(),
        );
        let mut last_stat_on: Instant = Instant::now();
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
                    NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        let (network_controller, _network_controller_) =
                            lock_for_read!(network_controller, LOCK_ORDER_NETWORK_CONTROLLER);
                        let sockets = network_controller.sockets.clone();
                        NetworkController::send_to_all(sockets, buffer, exceptions).await;
                        outgoing_messages.increment();
                    }
                    NetworkEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        buffer,
                    } => {
                        let (network_controller, _network_controller_) =
                            lock_for_read!(network_controller, LOCK_ORDER_NETWORK_CONTROLLER);
                        let sockets = network_controller.sockets.clone();
                        NetworkController::send_outgoing_message(sockets, index, buffer).await;
                        outgoing_messages.increment();
                    }
                    NetworkEvent::ConnectToPeer { peer_details } => {
                        NetworkController::connect_to_peer(
                            event_id,
                            network_controller.clone(),
                            peer_details,
                        )
                        .await;
                    }
                    NetworkEvent::PeerConnectionResult { .. } => {
                        unreachable!()
                    }
                    NetworkEvent::PeerDisconnected { peer_index: _ } => {
                        unreachable!()
                    }
                    NetworkEvent::IncomingNetworkMessage { .. } => {
                        unreachable!()
                    }
                    NetworkEvent::BlockFetchRequest {
                        block_hash,
                        peer_index,
                        url,
                    } => {
                        let sender;
                        let current_queries;
                        {
                            let (network_controller, _network_controller_) =
                                lock_for_read!(network_controller, LOCK_ORDER_NETWORK_CONTROLLER);

                            sender = network_controller.sender_to_saito_controller.clone();
                            current_queries = network_controller.currently_queried_urls.clone();
                        }
                        // starting new thread to stop io controller from getting blocked
                        tokio::spawn(async move {
                            NetworkController::fetch_block(
                                block_hash,
                                peer_index,
                                url,
                                event_id,
                                sender,
                                current_queries,
                            )
                            .await
                        });
                    }
                    NetworkEvent::BlockFetched { .. } => {
                        unreachable!()
                    }
                }
            }

            #[cfg(feature = "with-stats")]
            {
                let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);

                if Instant::now().duration_since(last_stat_on)
                    > Duration::from_millis(configs.get_server_configs().stat_timer_in_ms)
                {
                    last_stat_on = Instant::now();
                    outgoing_messages
                        .calculate_stats(TimeKeeper {}.get_timestamp_in_ms())
                        .await;
                    let (network_controller, _network_controller_) =
                        lock_for_read!(network_controller, LOCK_ORDER_NETWORK_CONTROLLER);

                    let stat = format!(
                        "--- stats ------ {} - capacity : {:?} / {:?}",
                        format!("{:width$}", "network::queue", width = 30),
                        network_controller.sender_to_saito_controller.capacity(),
                        network_controller.sender_to_saito_controller.max_capacity()
                    );
                    sender_to_stat.send(stat).await.unwrap();
                }
            }

            if !work_done {
                let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);

                tokio::time::sleep(Duration::from_millis(
                    configs.get_server_configs().thread_sleep_time_in_ms,
                ))
                .await;
            }
        }
    });
    let _result = tokio::join!(server_handle, controller_handle);
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
    io_controller: Arc<RwLock<NetworkController>>,
    port: u16,
    host: String,
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
                let _peer_counter = peer_counter.clone();
                let sender_to_io = sender_to_io.clone();
                let ws = ws.max_message_size(10_000_000_000);
                let ws = ws.max_frame_size(10_000_000_000);
                ws.on_upgrade(move |socket| async move {
                    debug!("socket connection established");
                    let (sender, receiver) = socket.split();

                    let (network_controller, _network_controller_) =
                        lock_for_read!(clone, LOCK_ORDER_NETWORK_CONTROLLER);

                    let peer_index;
                    {
                        let mut counter = network_controller.peer_counter.lock().await;
                        peer_index = counter.get_next_index();
                    }

                    NetworkController::send_new_peer(
                        0,
                        peer_index,
                        network_controller.sockets.clone(),
                        PeerSender::Warp(sender),
                        PeerReceiver::Warp(receiver),
                        sender_to_io,
                        None,
                    )
                    .await
                })
            });
        let http_route = warp::path!("block" / String).and_then(|block_hash: String| async move {
            debug!("serving block : {:?}", block_hash);
            let mut buffer: Vec<u8> = Default::default();
            let result = fs::read_dir(BLOCKS_DIR_PATH.to_string());
            if result.is_err() {
                debug!("no blocks found");
                return Err(warp::reject::not_found());
            }
            let paths: Vec<_> = result
                .unwrap()
                .map(|r| r.unwrap())
                .filter(|r| {
                    let filename = r.file_name().into_string().unwrap();
                    if !filename.contains(BLOCK_FILE_EXTENSION) {
                        return false;
                    }
                    if !filename.contains(block_hash.as_str()) {
                        return false;
                    }
                    debug!("selected file : {:?}", filename);
                    return true;
                })
                .collect();

            if paths.is_empty() {
                return Err(warp::reject::not_found());
            }
            let path = paths.first().unwrap();
            let file_path = BLOCKS_DIR_PATH.to_string()
                + "/"
                + path.file_name().into_string().unwrap().as_str();
            let result = File::open(file_path.as_str()).await;
            if result.is_err() {
                error!("failed opening file : {:?}", result.err().unwrap());
                todo!()
            }
            let mut file = result.unwrap();

            let result = file.read_to_end(&mut buffer).await;
            if result.is_err() {
                error!("failed reading file : {:?}", result.err().unwrap());
                todo!()
            }
            drop(file);

            let buffer_len = buffer.len();
            let result = Ok(warp::reply::with_status(buffer, StatusCode::OK));
            debug!("served block with : {:?} length", buffer_len);
            return result;
        });
        let routes = http_route.or(ws_route);
        // let (_, server) =
        //     warp::serve(ws_route).bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
        //         // tokio::signal::ctrl_c().await.ok();
        //     });
        // server.await;
        let address =
            SocketAddr::from_str((host + ":" + port.to_string().as_str()).as_str()).unwrap();
        warp::serve(routes).run(address).await;
    })
}

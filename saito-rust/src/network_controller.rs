use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::core::consensus::block::{Block, BlockType};
use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::consensus::peer_collection::PeerCollection;
use saito_core::core::defs::{
    BlockId, PrintForLog, SaitoHash, SaitoPublicKey, StatVariable, BLOCK_FILE_EXTENSION,
    STAT_BIN_COUNT,
};
use saito_core::core::io::network_event::NetworkEvent;
use saito_core::core::process::keep_time::KeepTime;
use saito_core::core::util;
use saito_core::core::util::configuration::{Configuration, PeerConfig};
use saito_core::lock_for_read;

use crate::io_event::IoEvent;
use crate::rust_io_handler::BLOCKS_DIR_PATH;
use crate::time_keeper::TimeKeeper;

// use crate::{IoEvent, NetworkEvent, TimeKeeper};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct NetworkController {
    sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    currently_queried_urls: Arc<Mutex<HashSet<String>>>,
    pub sender_to_saito_controller: Sender<IoEvent>,
}

impl NetworkController {
    pub async fn send(connection: &mut PeerSender, peer_index: u64, buffer: Vec<u8>) -> bool {
        let mut send_failed = false;
        // trace!(
        //     "sending buffer with size : {:?} to peer : {:?}",
        //     buffer.len(),
        //     peer_index
        // );
        // TODO : can be better optimized if we buffer the messages and flush once per timer event
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

    pub async fn send_outgoing_message(
        sockets: Arc<Mutex<HashMap<u64, PeerSender>>>,
        peer_index: u64,
        buffer: Vec<u8>,
    ) {
        let buf_len = buffer.len();
        // trace!(
        //     "sending outgoing message : peer = {:?} with size : {:?}",
        //     peer_index,
        //     buf_len
        // );
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
            warn!(
                "failed sending buffer of size : {:?} to peer : {:?}",
                buf_len, peer_index
            );
            sockets.remove(&peer_index);
        }
    }

    pub async fn connect_to_peer(
        event_id: u64,
        io_controller: Arc<RwLock<NetworkController>>,
        peer: util::configuration::PeerConfig,
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

            let network_controller = lock_for_read!(io_controller, LOCK_ORDER_NETWORK_CONTROLLER);

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

    pub async fn disconnect_from_peer(
        _event_id: u64,
        _io_controller: Arc<RwLock<NetworkController>>,
        _peer_id: u64,
    ) {
        info!("TODO : handle disconnect_from_peer")
    }
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
    pub async fn fetch_block(
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        event_id: u64,
        sender_to_core: Sender<IoEvent>,
        current_queries: Arc<Mutex<HashSet<String>>>,
        client: Client,
        block_id: BlockId,
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
        let block_fetch_timeout_in_ms = 10_000;
        let result = client
            .get(url.clone())
            .timeout(Duration::from_millis(block_fetch_timeout_in_ms))
            .send()
            .await;
        if result.is_err() {
            // TODO : should we retry here?
            warn!("failed fetching : {:?}", url);
            let mut queries = current_queries.lock().await;
            queries.remove(&url);
            sender_to_core
                .send(IoEvent {
                    event_processor_id: 1,
                    event_id,
                    event: NetworkEvent::BlockFetchFailed {
                        block_hash,
                        peer_index,
                        block_id,
                    },
                })
                .await
                .unwrap();
            return;
        }
        let response = result.unwrap();
        if !matches!(response.status(), StatusCode::OK) {
            warn!(
                "failed fetching block : {:?}, with error code : {:?} from url : {:?}",
                block_hash.to_hex(),
                response.status(),
                url
            );
            let mut queries = current_queries.lock().await;
            queries.remove(&url);
            sender_to_core
                .send(IoEvent {
                    event_processor_id: 1,
                    event_id,
                    event: NetworkEvent::BlockFetchFailed {
                        block_hash,
                        peer_index,
                        block_id,
                    },
                })
                .await
                .unwrap();
            return;
        }
        let result = response.bytes().await;
        if result.is_err() {
            warn!("failed getting byte buffer from fetching block : {:?}", url);
            let mut queries = current_queries.lock().await;
            queries.remove(&url);
            sender_to_core
                .send(IoEvent {
                    event_processor_id: 1,
                    event_id,
                    event: NetworkEvent::BlockFetchFailed {
                        block_hash,
                        peer_index,
                        block_id,
                    },
                })
                .await
                .unwrap();
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
                        // trace!(
                        //     "message buffer with size : {:?} received from peer : {:?}",
                        //     buffer.len(),
                        //     peer_index
                        // );
                        let message = IoEvent {
                            event_processor_id: 1,
                            event_id: 0,
                            event: NetworkEvent::IncomingNetworkMessage { peer_index, buffer },
                        };
                        sender.send(message).await.expect("sending failed");
                    } else if result.is_close() {
                        warn!("connection closed by remote peer : {:?}", peer_index);
                        NetworkController::send_peer_disconnect(sender, peer_index).await;
                        sockets.lock().await.remove(&peer_index);
                        break;
                    } else {
                        // warn!("unhandled type");
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
                            // trace!(
                            //     "message buffer with size : {:?} received from peer : {:?}",
                            //     buffer.len(),
                            //     peer_index
                            // );
                            let message = IoEvent {
                                event_processor_id: 1,
                                event_id: 0,
                                event: NetworkEvent::IncomingNetworkMessage { peer_index, buffer },
                            };
                            sender.send(message).await.expect("sending failed");
                        }
                        _ => {
                            // Not handling these scenarios
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
        self.counter += 1;
        self.counter
    }
}

///
///
/// # Arguments
///
/// * `receiver`:
/// * `sender_to_core`:
/// * `configs_lock`:
/// * `blockchain_lock`:
/// * `sender_to_stat`:
/// * `peers_lock`:
/// * `sender_to_network`: sender for this thread. only used for reading performance stats
///
/// returns: ()
///
/// # Examples
///
/// ```
///
/// ```
// TODO : refactor to use ProcessEvent trait
pub async fn run_network_controller(
    mut receiver: Receiver<IoEvent>,
    sender_to_core: Sender<IoEvent>,
    configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    blockchain_lock: Arc<RwLock<Blockchain>>,
    sender_to_stat: Sender<String>,
    peers_lock: Arc<RwLock<PeerCollection>>,
    sender_to_network: Sender<IoEvent>,
) {
    info!("running network handler");
    let peer_index_counter = Arc::new(Mutex::new(PeerCounter { counter: 0 }));

    let host;
    let url;
    let port;
    let public_key;
    {
        let configs = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);

        url = configs.get_server_configs().unwrap().host.clone()
            + ":"
            + configs
                .get_server_configs()
                .unwrap()
                .port
                .to_string()
                .as_str();
        port = configs.get_server_configs().unwrap().port;
        host = configs.get_server_configs().unwrap().host.clone();

        let blockchain = lock_for_read!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        let wallet = lock_for_read!(blockchain.wallet_lock, LOCK_ORDER_WALLET);
        public_key = wallet.public_key;
    }

    info!("starting server on : {:?}", url);
    let sender_clone = sender_to_core.clone();

    let network_controller_lock = Arc::new(RwLock::new(NetworkController {
        sockets: Arc::new(Mutex::new(HashMap::new())),
        sender_to_saito_controller: sender_to_core,
        peer_counter: peer_index_counter.clone(),
        currently_queried_urls: Arc::new(Default::default()),
    }));

    let server_handle = run_websocket_server(
        sender_clone.clone(),
        network_controller_lock.clone(),
        port,
        host,
        public_key,
        peers_lock,
    );

    let mut work_done = false;
    let controller_handle = tokio::spawn(async move {
        let mut outgoing_messages = StatVariable::new(
            "network::outgoing_msgs".to_string(),
            STAT_BIN_COUNT,
            sender_to_stat.clone(),
        );
        let stat_timer_in_ms;
        let thread_sleep_time_in_ms;
        {
            let configs_temp = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);
            stat_timer_in_ms = configs_temp.get_server_configs().unwrap().stat_timer_in_ms;
            thread_sleep_time_in_ms = configs_temp
                .get_server_configs()
                .unwrap()
                .thread_sleep_time_in_ms;
        }
        let io_pool = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(10)
            .enable_io()
            .enable_time()
            .thread_name("io-thread-pool")
            .build()
            .unwrap();

        let mut last_stat_on: Instant = Instant::now();
        loop {
            let result = receiver.recv().await;
            if result.is_some() {
                let event = result.unwrap();
                let event_id = event.event_id;
                let interface_event = event.event;
                work_done = true;
                match interface_event {
                    NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        let sockets;
                        {
                            let network_controller = lock_for_read!(
                                network_controller_lock,
                                LOCK_ORDER_NETWORK_CONTROLLER
                            );
                            sockets = network_controller.sockets.clone();
                        }

                        NetworkController::send_to_all(sockets, buffer, exceptions).await;
                        outgoing_messages.increment();
                    }
                    NetworkEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        buffer,
                    } => {
                        let sockets;
                        {
                            let network_controller = lock_for_read!(
                                network_controller_lock,
                                LOCK_ORDER_NETWORK_CONTROLLER
                            );
                            sockets = network_controller.sockets.clone();
                        }
                        NetworkController::send_outgoing_message(sockets, index, buffer).await;
                        outgoing_messages.increment();
                    }
                    NetworkEvent::ConnectToPeer { peer_details } => {
                        NetworkController::connect_to_peer(
                            event_id,
                            network_controller_lock.clone(),
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
                        block_id,
                    } => {
                        let sender;
                        let current_queries;
                        {
                            let network_controller = lock_for_read!(
                                network_controller_lock,
                                LOCK_ORDER_NETWORK_CONTROLLER
                            );

                            sender = network_controller.sender_to_saito_controller.clone();
                            current_queries = network_controller.currently_queried_urls.clone();
                        }
                        // starting new thread to stop io controller from getting blocked
                        io_pool.spawn(async move {
                            let client = reqwest::Client::new();

                            NetworkController::fetch_block(
                                block_hash,
                                peer_index,
                                url,
                                event_id,
                                sender,
                                current_queries,
                                client,
                                block_id,
                            )
                            .await
                        });
                    }
                    NetworkEvent::BlockFetched { .. } => {
                        unreachable!()
                    }
                    NetworkEvent::BlockFetchFailed { .. } => {
                        unreachable!()
                    }
                    NetworkEvent::DisconnectFromPeer { peer_index } => {
                        NetworkController::disconnect_from_peer(
                            event_id,
                            network_controller_lock.clone(),
                            peer_index,
                        )
                        .await
                    }
                }
            }

            #[cfg(feature = "with-stats")]
            {
                if Instant::now().duration_since(last_stat_on)
                    > Duration::from_millis(stat_timer_in_ms)
                {
                    last_stat_on = Instant::now();
                    outgoing_messages
                        .calculate_stats(TimeKeeper {}.get_timestamp_in_ms())
                        .await;
                    let network_controller =
                        lock_for_read!(network_controller_lock, LOCK_ORDER_NETWORK_CONTROLLER);

                    let stat = format!(
                        "{} - {} - capacity : {:?} / {:?}",
                        StatVariable::format_timestamp(TimeKeeper {}.get_timestamp_in_ms()),
                        format!("{:width$}", "network::queue_to_core", width = 40),
                        network_controller.sender_to_saito_controller.capacity(),
                        network_controller.sender_to_saito_controller.max_capacity()
                    );
                    sender_to_stat.send(stat).await.unwrap();

                    let stat = format!(
                        "{} - {} - capacity : {:?} / {:?}",
                        StatVariable::format_timestamp(TimeKeeper {}.get_timestamp_in_ms()),
                        format!("{:width$}", "network::queue_outgoing", width = 40),
                        sender_to_network.capacity(),
                        sender_to_network.max_capacity()
                    );
                    sender_to_stat.send(stat).await.unwrap();
                }
            }

            if !work_done {
                tokio::time::sleep(Duration::from_millis(thread_sleep_time_in_ms)).await;
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
    sender_clone: Sender<IoEvent>,
    io_controller: Arc<RwLock<NetworkController>>,
    port: u16,
    host: String,
    public_key: SaitoPublicKey,
    peers: Arc<RwLock<PeerCollection>>,
) -> JoinHandle<()> {
    info!("running websocket server on {:?}", port);
    tokio::spawn(async move {
        info!("starting websocket server");
        let io_controller = io_controller.clone();
        let sender_to_io = sender_clone.clone();
        let public_key = public_key.clone();
        let ws_route = warp::path("wsopen")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                debug!("incoming connection received");
                let clone = io_controller.clone();
                let sender_to_io = sender_to_io.clone();
                let ws = ws.max_message_size(10_000_000_000);
                let ws = ws.max_frame_size(10_000_000_000);
                ws.on_upgrade(move |socket| async move {
                    debug!("socket connection established");
                    let (sender, receiver) = socket.split();

                    let network_controller = lock_for_read!(clone, LOCK_ORDER_NETWORK_CONTROLLER);

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
                return Err(warp::reject::not_found());
            }
            let mut file = result.unwrap();

            let result = file.read_to_end(&mut buffer).await;
            if result.is_err() {
                error!("failed reading file : {:?}", result.err().unwrap());
                return Err(warp::reject::not_found());
            }
            drop(file);

            let buffer_len = buffer.len();
            let result = Ok(warp::reply::with_status(buffer, StatusCode::OK));
            debug!("served block with : {:?} length", buffer_len);
            return result;
        });

        // TODO : review this code
        let opt = warp::path::param::<String>()
            .map(Some)
            .or_else(|_| async { Ok::<(Option<String>,), std::convert::Infallible>((None,)) });
        let lite_route = warp::path!("lite-block" / String / ..)
            .and(opt)
            .and(warp::path::end())
            .and(warp::any().map(move || peers.clone()))
            .and_then(
                move |block_hash: String,
                      key: Option<String>,
                      peers: Arc<RwLock<PeerCollection>>| async move {
                    debug!("serving lite block : {:?}", block_hash);

                    let mut key1 = String::from("");
                    if key.is_some() {
                        key1 = key.unwrap();
                    } else {
                        warn!("key is not set to request lite blocks");
                        return Err(warp::reject::reject());
                    }

                    let key;
                    if key1.is_empty() {
                        key = public_key.clone();
                    } else {
                        let result;
                        if key1.len() == 66 {
                            result = SaitoPublicKey::from_hex(key1.as_str());
                            if result.is_err() {
                                warn!("key : {:?} couldn't be decoded", key1);
                                return Err(warp::reject::reject());
                            }
                        } else {
                            result = SaitoPublicKey::from_base58(key1.as_str());
                            if result.is_err() {
                                warn!("key : {:?} couldn't be decoded", key1);
                                return Err(warp::reject::reject());
                            }
                        }

                        let result = result.unwrap();
                        if result.len() != 33 {
                            warn!("key length : {:?} is not for public key", result.len());
                            return Err(warp::reject::reject());
                        }
                        key = result.try_into().unwrap();
                    }
                    let mut keylist;
                    {
                        let peers = lock_for_read!(peers, LOCK_ORDER_PEERS);
                        let peer = peers.find_peer_by_address(&key);
                        if peer.is_none() {
                            keylist = vec![key];
                        } else {
                            keylist = peer.as_ref().unwrap().key_list.clone();
                            keylist.push(key);
                        }
                    }

                    // let blockchain = lock_for_read!(blockchain, LOCK_ORDER_BLOCKCHAIN);

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
                        return Err(warp::reject::not_found());
                    }
                    let mut file = result.unwrap();

                    let result = file.read_to_end(&mut buffer).await;
                    if result.is_err() {
                        error!("failed reading file : {:?}", result.err().unwrap());
                        return Err(warp::reject::not_found());
                    }
                    drop(file);

                    let block = Block::deserialize_from_net(buffer);
                    if block.is_err() {
                        error!("failed parsing buffer into a block");
                        return Err(warp::reject::not_found());
                    }
                    let mut block = block.unwrap();
                    block.generate();
                    let block = block.generate_lite_block(keylist);
                    let buffer = block.serialize_for_net(BlockType::Full);
                    let buffer_len = buffer.len();
                    let result = Ok(warp::reply::with_status(buffer, StatusCode::OK));
                    debug!("served block with : {:?} length", buffer_len);
                    return result;
                    // }
                    // .await
                },
            );
        let routes = http_route.or(ws_route).or(lite_route);
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

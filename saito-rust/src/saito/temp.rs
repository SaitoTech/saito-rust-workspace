use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use tokio::net::TcpStream;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::common::defs::SaitoHash;
use saito_core::core::data;
use saito_core::core::data::block::BlockType;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::peer_collection::PeerCollection;

use crate::saito::rust_io_handler::RustIOHandler;
use crate::{IoEvent, NetworkEvent, ROUTING_EVENT_PROCESSOR_ID};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

use crate::saito::network_connection::NetworkConnection;

use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer::PeerConnection;
use saito_core::core::data::wallet::Wallet;

pub struct NetworkHandler {
    sockets: HashMap<u64, PeerSender>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    pub sender_to_saito_controller: Sender<IoEvent>,
    network: Arc<RwLock<Network>>,
}

impl NetworkHandler {
    pub async fn connect_to_peer(
        event_id: u64,
        io_controller: Arc<RwLock<NetworkHandler>>,
        config: data::configuration::PeerConfig,
        network: Arc<RwLock<Network>>,
    ) {
        // TODO : handle connecting to an already connected (via incoming connection) node.

        let mut protocol: String = String::from("ws");
        if config.protocol == "https" {
            protocol = String::from("wss");
        }
        let url = protocol
            + "://"
            + config.host.as_str()
            + ":"
            + config.port.to_string().as_str()
            + "/wsopen";
        debug!("connecting to peer : {:?}", url);

        let result = connect_async(url.clone()).await;
        if result.is_err() {
            warn!("failed connecting to peer : {:?}", config);
            //TODO : Retry connecting after an interval
            error!("{:?}", result.err());

            return;
        }

        debug!("connected to peer : {:?}", url);

        let result = result.unwrap();
        let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;
        let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) = socket.split();

        NetworkHandler::handle_new_peer_connection(
            io_controller.read().await.peer_counter.clone(),
            PeerSender::Tungstenite(socket_sender),
            PeerReceiver::Tungstenite(socket_receiver),
            Some(config),
            network,
        )
        .await;
    }

    pub async fn handle_new_peer_connection(
        peer_counter: Arc<Mutex<PeerCounter>>,
        sender: PeerSender,
        receiver: PeerReceiver,
        peer_data: Option<PeerConfig>,
        network: Arc<RwLock<Network>>,
    ) {
        let next_index;
        {
            let mut counter = peer_counter.lock().await;
            next_index = counter.get_next_index();
        }

        let mut connection = NetworkConnection::new(sender, next_index);
        {
            network
                .write()
                .await
                .handle_new_peer(peer_data, next_index, &mut connection)
                .await;
        }

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
                        network.write().await.handle_peer_disconnect(next_index);
                        break;
                    }

                    let result = result.unwrap();
                    if result.is_binary() {
                        let buffer = result.into_bytes();
                        let message = Message::deserialize(buffer);
                        if message.is_err() {
                            todo!()
                        }

                        NetworkHandler::process_incoming_message(
                            message.unwrap(),
                            &mut connection,
                            &network,
                        );
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
                        network.write().await.handle_peer_disconnect(next_index);
                        break;
                    }
                    let result = result.unwrap();
                    match result {
                        tungstenite::Message::Binary(buffer) => {
                            let message = Message::deserialize(buffer);
                            if message.is_err() {
                                todo!()
                            }

                            NetworkHandler::process_incoming_message(
                                message.unwrap(),
                                &mut connection,
                                &network,
                            );
                        }
                        _ => {
                            // Not handling these scenarios
                            todo!()
                        }
                    }
                },
            }
        });
    }

    async fn process_incoming_message(
        message: Message,
        connection: &mut NetworkConnection,
        network: &Arc<RwLock<Network>>,
    ) {
        let peer_index = connection.get_peer_index();

        debug!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            peer_index
        );

        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                network
                    .read()
                    .await
                    .handle_handshake_challenge(peer_index, challenge, connection)
                    .await;
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                network
                    .read()
                    .await
                    .handle_handshake_response(peer_index, response, connection)
                    .await;
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                network
                    .read()
                    .await
                    .handle_handshake_completion(peer_index, response, connection)
                    .await;
            }
            Message::BlockchainRequest(request) => {
                network
                    .read()
                    .await
                    .process_incoming_blockchain_request(request, peer_index, connection)
                    .await;
            }
            Message::BlockHeaderHash(hash) => {
                network
                    .read()
                    .await
                    .process_incoming_block_hash(hash, peer_index)
                    .await;
            }
            Message::ApplicationMessage(_) => {
                debug!("received buffer");
            }
            Message::Block(_) => {
                debug!("received block");
            }
            Message::Transaction(_) => {
                debug!("received transaction");
            }
        }
        debug!("incoming message processed");
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
        debug!("block buffer sent to blockchain controller");
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
    sender_to_controller: Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Wallet>>,
    peers: Arc<RwLock<PeerCollection>>,
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
    let peer_counter_clone = peer_index_counter.clone();
    let sender_clone = sender.clone();
    let network = Arc::new(RwLock::new(Network::new(
        Box::new(RustIOHandler::new(
            sender_to_controller.clone(),
            ROUTING_EVENT_PROCESSOR_ID,
        )),
        peers.clone(),
        blockchain.clone(),
        wallet.clone(),
        configs.clone(),
    )));

    let network_controller = Arc::new(RwLock::new(NetworkHandler {
        sockets: Default::default(),
        sender_to_saito_controller: sender,
        peer_counter: peer_index_counter.clone(),
        network: network.clone(),
    }));

    let network_controller_clone = network_controller.clone();

    let server_handle = run_websocket_server(
        peer_counter_clone,
        network_controller_clone.clone(),
        port,
        blockchain.clone(),
        network.clone(),
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
                    NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        trace!("waiting for the io controller write lock");
                        let mut io_controller = network_controller.write().await;
                        trace!("acquired the io controller write lock");
                        io_controller.send_to_all(buffer, exceptions).await;
                    }
                    NetworkEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        buffer,
                    } => {
                        trace!("waiting for the io_controller write lock");
                        let mut io_controller = network_controller.write().await;
                        trace!("acquired the io controller write lock");
                        //io_controller.send_outgoing_message(index, buffer).await;
                    }
                    NetworkEvent::ConnectToPeer { peer_details } => {
                        NetworkHandler::connect_to_peer(
                            event_id,
                            network_controller.clone(),
                            peer_details,
                            network.clone(),
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
                        {
                            let io_controller = network_controller.read().await;
                            sender = io_controller.sender_to_saito_controller.clone();
                        }
                        // starting new thread to stop io controller from getting blocked
                        tokio::spawn(async move {
                            NetworkHandler::fetch_block(
                                block_hash, peer_index, url, event_id, sender,
                            )
                            .await
                        });
                    }
                    NetworkEvent::BlockFetched { .. } => {
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
    io_controller: Arc<RwLock<NetworkHandler>>,
    port: u16,
    blockchain: Arc<RwLock<Blockchain>>,
    network: Arc<RwLock<Network>>,
) -> JoinHandle<()> {
    info!("running websocket server on {:?}", port);
    tokio::spawn(async move {
        info!("starting websocket server");
        let io_controller = io_controller.clone();
        let peer_counter = peer_counter.clone();
        let network = network.clone();

        let ws_route = warp::path("wsopen")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                debug!("incoming connection received");
                let clone = io_controller.clone();
                let peer_counter = peer_counter.clone();
                let network = network.clone();

                ws.on_upgrade(move |socket| async move {
                    debug!("socket connection established");
                    let (sender, receiver) = socket.split();

                    trace!("waiting for the io controller write lock");
                    let mut controller = clone.write().await;
                    trace!("acquired the io controller write lock");

                    NetworkHandler::handle_new_peer_connection(
                        peer_counter.clone(),
                        PeerSender::Warp(sender),
                        PeerReceiver::Warp(receiver),
                        None,
                        network.clone(),
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

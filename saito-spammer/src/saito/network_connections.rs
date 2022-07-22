use std::collections::HashMap;

use std::io::{Error, ErrorKind, Write};

use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use tokio::net::TcpStream;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

use saito_core::common::defs::SaitoHash;
use saito_core::core::data;
use saito_core::core::data::block::BlockType;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::wallet::Wallet;

use crate::saito::rust_io_handler::{FutureState, RustIOHandler};
use crate::saito::spam_generator::SpamGenerator;
use crate::{IoEvent, NetworkEvent};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct NetworkConnections {
    peers: Arc<RwLock<PeerCollection>>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
    sender_to_core: Sender<IoEvent>,
}

impl NetworkConnections {
    pub fn new(peers: Arc<RwLock<PeerCollection>>, sender_to_core: Sender<IoEvent>) -> Self {
        NetworkConnections {
            peers,
            peer_counter: Arc::new(Mutex::new(PeerCounter { counter: 0 })),
            senders: Arc::new(Mutex::new(HashMap::new())),
            sender_to_core,
        }
    }

    pub async fn connect_to_static_peer(&mut self, config: PeerConfig) {
        let mut counter = self.peer_counter.lock().await;
        let next_index = counter.get_next_index();

        Self::connect_to_peer(
            config,
            next_index,
            self.senders.clone(),
            self.sender_to_core.clone(),
        )
        .await;
    }

    fn get_connection_url(config: &PeerConfig) -> String {
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

        return url;
    }

    pub async fn connect_to_peer(
        config: PeerConfig,
        peer_index: u64,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
        sender_to_core: Sender<IoEvent>,
    ) {
        tokio::spawn(async move {
            let url = Self::get_connection_url(&config);

            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let result = connect_async(url.clone()).await;

                if result.is_ok() {
                    info!("connected to peer : {:?}", url);

                    let result = result.unwrap();
                    let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;
                    let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) =
                        socket.split();

                    tokio::join!(
                        Self::on_new_connection(
                            config.clone(),
                            peer_index,
                            senders.clone(),
                            sender_to_core.clone(),
                            socket_receiver,
                            socket_sender,
                        )
                        .await
                    );
                } else {
                    warn!(
                        "failed connecting to peer : {:?}, error : {:?}, reconnecting ...",
                        url,
                        result.err()
                    );
                }
            }
        });
    }

    pub async fn on_new_connection(
        config: PeerConfig,
        peer_index: u64,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
        sender_to_core: Sender<IoEvent>,
        mut receiver: SocketReceiver,
        sender: SocketSender,
    ) -> JoinHandle<()> {
        senders.lock().await.insert(peer_index, sender);

        sender_to_core
            .send(IoEvent {
                event_processor_id: 1,
                event_id: 1,
                event: NetworkEvent::PeerConnectionResult {
                    peer_details: Some(config),
                    result: Ok(peer_index),
                },
            })
            .await
            .expect("sending failed");

        tokio::spawn(async move {
            debug!("new thread started for peer receiving");

            loop {
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
                    tungstenite::Message::Binary(buffer) => {
                        let message = IoEvent {
                            event_processor_id: 1,
                            event_id: 0,
                            event: NetworkEvent::IncomingNetworkMessage { peer_index, buffer },
                        };
                        sender_to_core.send(message).await.expect("sending failed");
                    }
                    _ => {
                        // Not handling these scenarios
                        todo!()
                    }
                }
            }

            //Connection has exited the loop, which means disconnected
            senders.lock().await.remove(&peer_index);
        })
    }

    async fn send(
        &self,
        peer_index: u64,
        buffer: Vec<u8>,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        let mut connections = self.senders.lock().await;
        let mut sender = connections.get_mut(&peer_index).unwrap();

        let result = sender.send(tungstenite::Message::Binary(buffer)).await;

        if result.is_err() {
            return Err(result.err().unwrap());
        }

        return sender.flush().await;
    }

    async fn send_to_all(&self, buffer: Vec<u8>, exceptions: Vec<u64>) {
        let mut connections = self.senders.lock().await;

        for entry in connections.iter_mut() {
            let peer_index = entry.0;

            if !exceptions.contains(peer_index) {
                let sender = entry.1;
                let result = sender
                    .send(tungstenite::Message::Binary(buffer.clone()))
                    .await;

                if result.is_ok() {
                    sender.flush().await;
                }
            }
        }
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
            error!("Block fetching error : {:?}", result.err().unwrap());
            return;
        }
        let response = result.unwrap();
        let result = response.bytes().await;
        if result.is_err() {
            error!(
                "Error converting fetched blocked into binary : {:?}",
                result.err().unwrap()
            );
            return;
        }
        let result = result.unwrap();
        let buffer = result.to_vec();
        debug!("block buffer received");

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

pub async fn run_network_controller(
    mut receiver: Receiver<IoEvent>,
    sender_to_core: Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    peers: Arc<RwLock<PeerCollection>>,
) {
    info!("running network handler");

    let network_connections = Arc::new(RwLock::new(NetworkConnections::new(peers, sender_to_core)));

    let mut work_done = false;
    let controller_handle = tokio::spawn(async move {
        loop {
            let result = receiver.recv().await;
            if result.is_some() {
                let event = result.unwrap();
                let event_id = event.event_id;
                let interface_event = event.event;
                work_done = true;
                match interface_event {
                    NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        trace!("waiting for the io_controller write lock");
                        let mut instance = network_connections.write().await;
                        trace!("acquired the io controller write lock");
                        instance.send_to_all(buffer, exceptions);
                    }
                    NetworkEvent::OutgoingNetworkMessage {
                        peer_index: index,
                        buffer,
                    } => {
                        trace!("waiting for the io_controller write lock");
                        let mut instance = network_connections.write().await;
                        trace!("acquired the io controller write lock");
                        instance.send(index, buffer).await;
                    }
                    NetworkEvent::ConnectToPeer { peer_details } => {
                        let mut instance = network_connections.write().await;
                        instance.connect_to_static_peer(peer_details).await;
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
                            let io_controller = network_connections.read().await;
                            sender = io_controller.sender_to_core.clone();
                        }
                        // starting new thread to stop io controller from getting blocked
                        tokio::spawn(async move {
                            NetworkConnections::fetch_block(
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
    let _result = tokio::join!(controller_handle);
}

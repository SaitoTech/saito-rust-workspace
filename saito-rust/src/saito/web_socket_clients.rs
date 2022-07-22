use crate::saito::consensus_handler::ConsensusEvent;
use crate::saito::network_handler::NetworkHandler;
use crate::IoEvent;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use saito_core::common::command::NetworkEvent;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;
type SocketSenderMap = HashMap<u64, Arc<Mutex<SocketSender>>>;

pub struct WebSocketClients {}

impl WebSocketClients {
    pub async fn Run(
        configs: Arc<RwLock<Configuration>>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        let peer_configs = configs.read().await.peers.clone();
        let mut peers = peers.write().await;
        let sender_map: Arc<RwLock<SocketSenderMap>> = Default::default();

        let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);

        for config in peer_configs {
            let peer = peers.add(event_sender.clone(), Some(config)).await;
            Self::connect_to_peer(peer, sender_to_core.clone());
        }

        Self::run_event_loop(event_receiver, sender_map, sender_to_core).await
    }

    async fn connect_to_peer(
        peer: Arc<RwLock<Peer>>,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let url;
                {
                    url = peer.read().await.get_connection_url().unwrap();
                }

                let result = connect_async(url.clone()).await;

                if result.is_ok() {
                    info!("connected to peer : {:?}", url);

                    let result = result.unwrap();
                    let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;
                    let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) =
                        socket.split();

                    let socket_sender = Arc::new(Mutex::new(socket_sender));

                    Self::handle_new_peer_connection(
                        peer.clone(),
                        socket_sender,
                        socket_receiver,
                        sender_to_core,
                    )
                    .await;

                    break; // Exit the reconnection loop
                } else {
                    warn!(
                        "failed connecting to peer : {:?}, error : {:?}, reconnecting ...",
                        url,
                        result.err()
                    );
                }
            }
        })
    }

    async fn handle_new_peer_connection(
        peer: Arc<RwLock<Peer>>,
        sender: Arc<Mutex<SocketSender>>,
        mut receiver: SocketReceiver,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        let peer_index;
        {
            // This is a client connection, the server side will initiate the handshake
            peer_index = peer.read().await.peer_index;
        }

        info!("Handling new peer connection : {:?}", peer_index);

        tokio::spawn(async move {
            debug!("new thread started for peer receiving");
            loop {
                let result = receiver.next().await;
                if result.is_none() {
                    continue;
                }
                let result = result.unwrap();
                if result.is_err() {
                    error!(
                        "failed receiving message from server, {:?} : {:?}",
                        peer_index,
                        result.err().unwrap()
                    );
                    Self::connect_to_peer(peer, sender_to_core.clone());
                    break; // Exiting the receiving loop
                }
                let result = result.unwrap();
                match result {
                    tungstenite::Message::Binary(buffer) => {
                        let message = Message::deserialize(buffer);

                        if message.is_ok() {
                            Self::process_incoming_message(&peer, message.unwrap(), &sender);
                        } else {
                            error!(
                                "Deserializing incoming message failed, reason {:?}",
                                message.err().unwrap()
                            );
                        }
                    }
                    _ => {
                        // Not handling these scenarios
                        todo!()
                    }
                }
            }
        })
    }

    async fn process_incoming_message(
        peer: &Arc<RwLock<Peer>>,
        message: Message,
        sender: &Arc<Mutex<SocketSender>>,
    ) {
        let result;
        {
            result = peer.write().await.process_incoming_message(message).await;
        }

        if result.is_ok() {
            let buffer = result.unwrap();

            if buffer.len() > 0 {
                Self::send(sender, buffer);
            }
        } else {
            warn!("Handling input from peer failed, {:?}", result.err());
        }

        debug!("incoming message processed");
    }

    async fn run_event_loop(
        mut event_receiver: Receiver<NetworkEvent>,
        sender_map: Arc<RwLock<SocketSenderMap>>,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let result = event_receiver.recv().await;

                if result.is_some() {
                    let event = result.unwrap();

                    match event {
                        NetworkEvent::OutgoingNetworkMessage { peer_index, buffer } => {
                            let sender_map = sender_map.read().await;
                            let sender = sender_map.get(&peer_index);

                            if sender.is_some() {
                                Self::send(sender.unwrap(), buffer);
                            } else {
                                warn!("Trying to send to an unknown peer : {:?}", peer_index);
                            }
                        }
                        NetworkEvent::BlockFetchRequest {
                            block_hash,
                            peer_index,
                            url,
                            request_id,
                        } => {
                            // starting new thread to stop io controller from getting blocked
                            let sender = sender_to_core.clone();
                            tokio::spawn(async move {
                                NetworkHandler::fetch_block(
                                    block_hash, peer_index, url, request_id, sender,
                                )
                                .await
                            });
                        }
                        NetworkEvent::BlockFetched { .. } => {
                            unreachable!()
                        }
                    }
                }
            }
        })
    }

    async fn send(sender: &Arc<Mutex<SocketSender>>, buffer: Vec<u8>) {
        let mut sender = sender.lock().await;

        sender
            .send(tungstenite::Message::Binary(buffer))
            .await
            .unwrap();

        sender.flush().await.unwrap();
    }
}

use crate::saito::block_fetching_task::BlockFetchingTaskRunner;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{error, info, warn};
use saito_core::common::command::NetworkEvent;
use saito_core::core::data::context::Context;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct WebSocketClients {
    context: Context,
    peers: Arc<RwLock<PeerCollection>>,
    task_runner: Arc<BlockFetchingTaskRunner>,
}

impl WebSocketClients {
    pub fn new(
        context: Context,
        peers: Arc<RwLock<PeerCollection>>,
        task_runner: Arc<BlockFetchingTaskRunner>,
    ) -> Self {
        WebSocketClients {
            context,
            peers,
            task_runner,
        }
    }

    pub async fn connect(&self) -> Vec<JoinHandle<()>> {
        let peer_configs = self.context.configuration.read().await.peers.clone();
        let senders: Arc<Mutex<HashMap<u64, SocketSender>>> = Default::default();
        let mut handles: Vec<JoinHandle<()>> = Default::default();

        for config in peer_configs {
            let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);

            let peer;
            {
                let mut peers = self.peers.write().await;
                peer = peers.add(&self.context, event_sender, Some(config)).await;
            }

            handles.push(
                Self::process_network_events(
                    event_receiver,
                    senders.clone(),
                    self.task_runner.clone(),
                )
                .await,
            );
            handles.push(Self::connect_to_peer(peer.clone(), senders.clone()).await);
        }

        return handles;
    }

    async fn connect_to_peer(
        peer: Arc<RwLock<Peer>>,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let url;
                let peer_index;
                {
                    let peer = peer.read().await;
                    url = peer.get_connection_url().unwrap();
                    peer_index = peer.peer_index;
                }

                let result = connect_async(url.clone()).await;

                if result.is_ok() {
                    info!("connected to peer : {:?}", url);

                    let result = result.unwrap();
                    let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;
                    let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) =
                        socket.split();

                    senders.lock().await.insert(peer_index, socket_sender);
                    Self::receive_incoming_messages(peer.clone(), socket_receiver).await;
                    senders.lock().await.remove(&peer_index);
                    error!("Disconnected from peer {:?}, reconnecting ...", peer_index);
                } else {
                    warn!(
                        "failed connecting to peer : {:?}, error : {:?}, reconnecting ...",
                        url,
                        result.err()
                    );
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }

    async fn receive_incoming_messages(
        peer: Arc<RwLock<Peer>>,
        mut socket_receiver: SocketReceiver,
    ) {
        loop {
            if let Some(input) = socket_receiver.next().await {
                match input {
                    Ok(tungstenite_message) => {
                        if let tungstenite::Message::Binary(buffer) = tungstenite_message {
                            if peer
                                .write()
                                .await
                                .process_incoming_data(buffer)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        let peer = peer.read().await;
                        error!(
                            "failed receiving message from server, {:?} : {:?}",
                            peer.peer_index, error
                        );

                        peer.send_event(NetworkEvent::PeerDisconnected {
                            peer_index: peer.peer_index,
                        })
                        .await;

                        break; // Exiting the receiving loop
                    }
                }
            }
        }
    }

    async fn process_network_events(
        mut event_receiver: Receiver<NetworkEvent>,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
        task_runner: Arc<BlockFetchingTaskRunner>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Some(event) = event_receiver.recv().await {
                    match event {
                        NetworkEvent::OutgoingNetworkMessage { peer_index, buffer } => {
                            let senders = &mut senders.lock().await;
                            if let Some(socket_sender) = senders.get_mut(&peer_index) {
                                let mut message_sent = false;

                                if socket_sender
                                    .send(tungstenite::Message::Binary(buffer))
                                    .await
                                    .is_ok()
                                {
                                    if socket_sender.flush().await.is_ok() {
                                        message_sent = true;
                                    }
                                }

                                if !message_sent {
                                    senders.remove(&peer_index);
                                }
                            }
                        }
                        NetworkEvent::BlockFetchRequest {
                            block_hash: _,
                            peer_index: _,
                            url,
                            request_id: _,
                        } => {
                            task_runner.run_task(url).await;
                        }
                        NetworkEvent::BlockFetched { .. } => {
                            unreachable!()
                        }
                        NetworkEvent::PeerDisconnected { peer_index } => {
                            let mut senders = senders.lock().await;
                            senders.remove(&peer_index);
                        }
                    }
                }
            }
        })
    }
}

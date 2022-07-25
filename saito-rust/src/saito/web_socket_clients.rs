use crate::saito::block_fetching_task::BlockFetchingTaskRunner;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use saito_core::common::command::NetworkEvent;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct WebSocketClients {
    configs: Arc<RwLock<Configuration>>,
    peers: Arc<RwLock<PeerCollection>>,
    task_runner: Arc<BlockFetchingTaskRunner>,
}

impl WebSocketClients {
    pub fn new(context: &Context, sender_to_miner: Sender<MiningEvent>) -> Self {
        WebSocketClients {
            configs: context.configuration.clone(),
            peers: context.peers.clone(),
            task_runner: Arc::new(BlockFetchingTaskRunner {
                peers: context.peers.clone(),
                blockchain: context.blockchain.clone(),
                sender_to_miner,
            }),
        }
    }

    pub async fn connect(&self) {
        let peer_configs = self.configs.read().await.peers.clone();

        for config in peer_configs {
            let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);

            let peer;
            {
                let mut peers = self.peers.write().await;
                peer = peers.add(event_sender, Some(config)).await;
            }

            Self::connect_to_peer(peer, event_receiver, self.task_runner.clone()).await;
        }
    }

    async fn connect_to_peer(
        peer: Arc<RwLock<Peer>>,
        event_receiver: Receiver<NetworkEvent>,
        task_runner: Arc<BlockFetchingTaskRunner>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

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

                    Self::receive_incoming_messages(peer_index, peer.clone(), socket_receiver)
                        .await;
                    Self::process_network_events(peer, event_receiver, socket_sender, task_runner)
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

    async fn receive_incoming_messages(
        peer_index: u64,
        peer: Arc<RwLock<Peer>>,
        mut socket_receiver: SocketReceiver,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            debug!("new thread started for peer receiving");
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
                            error!(
                                "failed receiving message from server, {:?} : {:?}",
                                peer_index, error
                            );
                            peer.read()
                                .await
                                .send_event(NetworkEvent::PeerDisconnected { peer_index })
                                .await;
                            break; // Exiting the receiving loop
                        }
                    }
                }
            }
        })
    }

    async fn process_network_events(
        peer: Arc<RwLock<Peer>>,
        mut event_receiver: Receiver<NetworkEvent>,
        mut socket_sender: SocketSender,
        task_runner: Arc<BlockFetchingTaskRunner>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Some(event) = event_receiver.recv().await {
                    match event {
                        NetworkEvent::OutgoingNetworkMessage {
                            peer_index: _,
                            buffer,
                        } => {
                            socket_sender
                                .send(tungstenite::Message::Binary(buffer))
                                .await
                                .unwrap();

                            socket_sender.flush().await.unwrap();
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
                        NetworkEvent::PeerDisconnected { .. } => {
                            //Self::connect_to_peer(peer, event_receiver, task_runner).await;
                            break;
                        }
                    }
                }
            }
        })
    }
}

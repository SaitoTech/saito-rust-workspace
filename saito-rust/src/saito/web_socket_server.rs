use crate::saito::block_fetching_task::BlockFetchingTaskRunner;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace};
use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::SaitoHash;
use saito_core::core::data::block::BlockType;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

type ServerSideSender = SplitSink<WebSocket, warp::ws::Message>;
type ServerSideReceiver = SplitStream<WebSocket>;

pub struct WebSocketServer {
    configs: Arc<RwLock<Configuration>>,
    peers: Arc<RwLock<PeerCollection>>,
    blockchain: Arc<RwLock<Blockchain>>,
    task_runner: Arc<BlockFetchingTaskRunner>,
}

impl WebSocketServer {
    pub fn new(context: &Context, sender_to_miner: Sender<MiningEvent>) -> Self {
        WebSocketServer {
            configs: context.configuration.clone(),
            peers: context.peers.clone(),
            blockchain: context.blockchain.clone(),
            task_runner: Arc::new(BlockFetchingTaskRunner {
                peers: context.peers.clone(),
                blockchain: context.blockchain.clone(),
                sender_to_miner,
            }),
        }
    }

    pub async fn run(&self) -> JoinHandle<()> {
        let peers = self.peers.clone();
        let blockchain = self.blockchain.clone();
        let task_runner = self.task_runner.clone();
        let port;
        {
            let configs = self.configs.read().await;
            port = configs.server.port;
        }

        tokio::spawn(async move {
            Self::run_ws_server(peers, blockchain, port, task_runner).await;
        })
    }

    async fn run_ws_server(
        peers: Arc<RwLock<PeerCollection>>,
        blockchain: Arc<RwLock<Blockchain>>,
        port: u16,
        task_runner: Arc<BlockFetchingTaskRunner>,
    ) {
        info!("Starting Web Socket Server on {:?}", port);
        let peers = peers.clone();
        let task_runner = task_runner.clone();

        let ws_route = warp::path("wsopen")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                debug!("incoming connection received");
                let peers = peers.clone();
                let task_runner = task_runner.clone();

                ws.on_upgrade(move |socket| async move {
                    debug!("socket connection established");

                    let peers = peers.clone();
                    let task_runner = task_runner.clone();

                    let (sender, receiver) = socket.split();
                    let (event_sender, event_receiver) =
                        tokio::sync::mpsc::channel::<NetworkEvent>(1000);

                    let new_peer;
                    {
                        new_peer = peers.write().await.add(event_sender, None).await;
                    }

                    Self::process_network_events(event_receiver, sender, task_runner).await;
                    Self::receive_incoming_messages(new_peer, receiver).await;
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
        warp::serve(routes).run(([127, 0, 0, 1], port)).await;
    }

    async fn receive_incoming_messages(
        peer: Arc<RwLock<Peer>>,
        mut receiver: ServerSideReceiver,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            debug!("new thread started for peer receiving");

            let peer_index;
            {
                let mut peer = peer.write().await;
                peer_index = peer.peer_index;
                info!("Handling new peer connection : {:?}", peer_index);

                if let Err(error) = peer.initiate_handshake().await {
                    error!("Initializing handshake failed, {:?}", error);
                    peer.send_event(NetworkEvent::PeerDisconnected { peer_index })
                        .await;
                    return;
                }
            }

            loop {
                if let Some(input) = receiver.next().await {
                    match input {
                        Ok(web_socket_message) => {
                            if web_socket_message.is_binary() {
                                if peer
                                    .write()
                                    .await
                                    .process_incoming_data(web_socket_message.into_bytes())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            } else {
                                error!("Input is not binary {:?}", peer_index);
                            }
                        }
                        Err(error) => {
                            error!(
                                "failed receiving message from client, {:?} : {:?}",
                                peer_index, error
                            );
                            peer.read()
                                .await
                                .send_event(NetworkEvent::PeerDisconnected { peer_index })
                                .await;
                            break;
                        }
                    }
                }
            }
        })
    }

    async fn process_network_events(
        mut event_receiver: Receiver<NetworkEvent>,
        mut sender: ServerSideSender,
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
                            sender
                                .send(warp::ws::Message::binary(buffer))
                                .await
                                .unwrap();
                            sender.flush().await.unwrap();
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
                        NetworkEvent::PeerDisconnected { peer_index: _ } => {
                            break;
                        }
                    }
                }
            }
        })
    }
}

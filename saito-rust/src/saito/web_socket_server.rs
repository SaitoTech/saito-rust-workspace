use crate::saito::consensus_handler::ConsensusEvent;
use crate::saito::network_handler::NetworkHandler;
use crate::IoEvent;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::SaitoHash;
use saito_core::core::data::block::BlockType;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::peer::Peer;
use saito_core::core::data::peer_collection::PeerCollection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use warp::http::StatusCode;
use warp::ws::WebSocket;
use warp::Filter;

type ServerSideSender = SplitSink<WebSocket, warp::ws::Message>;
type ServerSideReceiver = SplitStream<WebSocket>;
type ServerSideSenderMap = HashMap<u64, Arc<Mutex<ServerSideSender>>>;

pub struct WebSocketServer {}

impl WebSocketServer {
    pub async fn run(
        peers: Arc<RwLock<PeerCollection>>,
        port: u16,
        blockchain: Arc<RwLock<Blockchain>>,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        let (event_sender, event_receiver) = tokio::sync::mpsc::channel::<NetworkEvent>(1000);
        let sender_map: Arc<RwLock<ServerSideSenderMap>> = Default::default();

        Self::run_event_loop(event_receiver, sender_map.clone(), sender_to_core.clone());

        info!("running websocket server on {:?}", port);
        tokio::spawn(async move {
            info!("starting websocket server");

            let peers = peers.clone();
            let sender_to_core = sender_to_core.clone();
            let event_sender = event_sender.clone();
            let sender_map = sender_map.clone();

            let ws_route = warp::path("wsopen")
                .and(warp::ws())
                .map(move |ws: warp::ws::Ws| {
                    debug!("incoming connection received");
                    let peers = peers.clone();
                    let sender_to_core = sender_to_core.clone();
                    let event_sender = event_sender.clone();
                    let sender_map = sender_map.clone();

                    ws.on_upgrade(move |socket| async move {
                        debug!("socket connection established");
                        let (sender, receiver) = socket.split();
                        let sender = Arc::new(Mutex::new(sender));
                        let event_sender = event_sender.clone();
                        let sender_map = sender_map.clone();

                        let new_peer;
                        {
                            new_peer = peers.write().await.add(event_sender, None).await;
                        }
                        {
                            sender_map
                                .write()
                                .await
                                .insert(new_peer.read().await.peer_index, sender.clone());
                        }
                        {
                            Self::handle_new_peer_connection(
                                new_peer,
                                sender,
                                receiver,
                                sender_to_core,
                            )
                            .await;
                        }
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
        })
    }

    async fn handle_new_peer_connection(
        peer: Arc<RwLock<Peer>>,
        sender: Arc<Mutex<ServerSideSender>>,
        mut receiver: ServerSideReceiver,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let peer_index;
            {
                let mut peer = peer.write().await;
                peer_index = peer.peer_index;
                info!("Handling new peer connection : {:?}", peer_index);

                let result = peer.initiate_handshake().await;

                if result.is_ok() {
                    Self::send(&sender, result.unwrap());
                } else {
                    error!("Initializing handshake failed")
                }
            }

            debug!("new thread started for peer receiving");
            loop {
                let result = receiver.next().await;

                if result.is_none() {
                    continue;
                }

                let result = result.unwrap();
                if result.is_err() {
                    warn!(
                        "failed receiving message from client, {:?} : {:?}",
                        peer_index,
                        result.err().unwrap()
                    );
                    break;
                }

                let result = result.unwrap();
                if result.is_binary() {
                    let buffer = result.into_bytes();
                    let message = Message::deserialize(buffer);

                    if message.is_ok() {
                        Self::process_incoming_message(&peer, message.unwrap(), &sender);
                    } else {
                        error!(
                            "Deserializing incoming message failed, reason {:?}",
                            message.err().unwrap()
                        );
                    }
                } else {
                    todo!()
                }
            }
        })
    }

    async fn process_incoming_message(
        peer: &Arc<RwLock<Peer>>,
        message: Message,
        sender: &Arc<Mutex<ServerSideSender>>,
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
        sender_map: Arc<RwLock<ServerSideSenderMap>>,
        sender_to_core: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let result = event_receiver.recv().await;
                if result.is_some() {
                    let event = result.unwrap();

                    match event {
                        NetworkEvent::OutgoingNetworkMessage { peer_index, buffer } => {
                            Self::send_outgoing_message(&sender_map, &peer_index, buffer).await
                        }
                        NetworkEvent::BlockFetchRequest {
                            block_hash,
                            peer_index,
                            url,
                            request_id,
                        } => {
                            Self::fetch_block(
                                &sender_to_core,
                                block_hash,
                                peer_index,
                                url,
                                request_id,
                            )
                            .await;
                        }
                        NetworkEvent::BlockFetched { .. } => {
                            unreachable!()
                        }
                    }
                }
            }
        })
    }

    async fn fetch_block(
        sender_to_core: &Sender<ConsensusEvent>,
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        request_id: u64,
    ) {
        // starting new thread to stop io controller from getting blocked
        let sender = sender_to_core.clone();
        tokio::spawn(async move {
            NetworkHandler::fetch_block(block_hash, peer_index, url, request_id, sender).await
        });
    }

    async fn send_outgoing_message(
        sender_map: &Arc<RwLock<ServerSideSenderMap>>,
        peer_index: &u64,
        buffer: Vec<u8>,
    ) {
        let sender_map = sender_map.read().await;
        let sender = sender_map.get(&peer_index);

        if sender.is_some() {
            Self::send(sender.unwrap(), buffer);
        } else {
            warn!("Trying to send to an unknown peer : {:?}", peer_index);
        }
    }

    async fn send(sender: &Arc<Mutex<ServerSideSender>>, buffer: Vec<u8>) {
        let mut sender = sender.lock().await;
        sender
            .send(warp::ws::Message::binary(buffer))
            .await
            .unwrap();
        sender.flush().await.unwrap();
    }
}

use std::collections::HashMap;

use std::io::{Error, ErrorKind, Write};

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
use saito_core::core::data::peer::{Peer, PeerConnection};
use saito_core::core::data::peer_collection::PeerCollection;

use crate::saito::rust_io_handler::{FutureState, RustIOHandler};
use crate::{IoEvent, NetworkEvent};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

use crate::saito::network_controller::{PeerReceiver, PeerSender};
use async_trait::async_trait;
use saito_core::core::data::msg::message::Message;

pub struct NetworkConnection {
    sender: PeerSender,
    peer: Peer,
}

impl NetworkConnection {
    pub fn new(sender: PeerSender, peer: Peer) -> NetworkConnection {
        NetworkConnection { sender, peer }
    }

    pub async fn receive_messages(&mut self, receiver: PeerReceiver) {
        debug!(
            "starting new task for reading from peer : {:?}",
            self.peer.peer_index
        );

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
                        //NetworkController::send_peer_disconnect(sender, peer_index).await;
                        break;
                    }

                    let message = result.unwrap();
                    if result.is_binary() {
                        let buffer = result.into_bytes();
                        let message = Message::deserialize(buffer);
                        if message.is_err() {
                            todo!()
                        }

                        self.process_incoming_message(message.unwrap());
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
                        //NetworkController::send_peer_disconnect(sender, peer_index).await;
                        break;
                    }
                    let result = result.unwrap();
                    match result {
                        tungstenite::Message::Binary(buffer) => {
                            let message = Message::deserialize(buffer);
                            if message.is_err() {
                                todo!()
                            }

                            self.process_incoming_message(message.unwrap());
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

    async fn process_incoming_message(&mut self, message: Message) {
        debug!(
            "processing incoming message type : {:?} from peer : {:?}",
            message.get_type_value(),
            self.peer.peer_index
        );

        match message {
            Message::HandshakeChallenge(challenge) => {
                debug!("received handshake challenge");
                self.peer.handle_handshake_challenge(challenge, &self).await;
            }
            Message::HandshakeResponse(response) => {
                debug!("received handshake response");
                self.peer.handle_handshake_response(response, &self).await;
            }
            Message::HandshakeCompletion(response) => {
                debug!("received handshake completion");
                self.peer.handle_handshake_completion(response).await;
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
            Message::BlockchainRequest(request) => {
                self.network
                    .process_incoming_blockchain_request(
                        request,
                        peer_index,
                        self.blockchain.clone(),
                    )
                    .await;
            }
            Message::BlockHeaderHash(hash) => {
                self.network
                    .process_incoming_block_hash(hash, peer_index, self.blockchain.clone())
                    .await;
            }
        }
        debug!("incoming message processed");
    }
}

#[async_trait]
impl PeerConnection for NetworkConnection {
    async fn send_message(&mut self, buffer: Vec<u8>) {
        debug!(
            "sending outgoing message : peer = {:?}",
            self.peer.peer_index
        );

        let mut sender = &self.sender;
        if sender.is_none() {
            todo!()
        }

        match sender {
            PeerSender::Warp(mut sender) => {
                sender
                    .send(warp::ws::Message::binary(buffer.clone()))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
            PeerSender::Tungstenite(mut sender) => {
                sender
                    .send(tungstenite::Message::Binary(buffer.clone()))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
        }
    }
}

use std::borrow::BorrowMut;
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
use saito_core::core::data::peer_collection::PeerCollection;

use crate::saito::rust_io_handler::{FutureState, RustIOHandler};
use crate::{IoEvent, NetworkEvent};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

use crate::saito::network_handler::{PeerReceiver, PeerSender};
use async_trait::async_trait;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer::PeerConnection;

pub struct NetworkConnection {
    sender: PeerSender,
    pub(crate) peer_index: u64,
}

impl NetworkConnection {
    pub fn new(sender: PeerSender, peer_index: u64) -> NetworkConnection {
        NetworkConnection { sender, peer_index }
    }
}

#[async_trait]
impl PeerConnection for NetworkConnection {
    async fn send_message(&mut self, buffer: Vec<u8>) -> Result<(), Error> {
        debug!("sending outgoing message : peer = {:?}", self.peer_index);

        match self.sender {
            PeerSender::Warp(ref mut sender) => {
                sender
                    .send(warp::ws::Message::binary(buffer.clone()))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
            PeerSender::Tungstenite(ref mut sender) => {
                sender
                    .send(tungstenite::Message::Binary(buffer.clone()))
                    .await
                    .unwrap();
                sender.flush().await.unwrap();
            }
        }

        Ok(())
    }

    fn get_peer_index(&self) -> u64 {
        self.peer_index
    }
}

use log::{debug, info, trace};
use std::sync::Arc;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use saito_core::common::defs::SaitoHash;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::peer_collection::PeerCollection;

use crate::saito::rust_io_handler::RustIOHandler;
use crate::{IoEvent, NetworkEvent};

use crate::saito::consensus_handler::ConsensusEvent;
use crate::saito::web_socket_clients::WebSocketClients;
use crate::saito::web_socket_server::WebSocketServer;
use saito_core::core::data::wallet::Wallet;

pub struct NetworkHandler {}

impl NetworkHandler {
    pub async fn run(
        mut receiver: Receiver<IoEvent>,
        sender_to_consensus_handler: Sender<ConsensusEvent>,
        configs: Arc<RwLock<Configuration>>,
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        peers: Arc<RwLock<PeerCollection>>,
    ) {
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

        let server_handle = WebSocketServer::run(
            peers.clone(),
            port,
            blockchain,
            sender_to_consensus_handler.clone(),
        )
        .await;
        WebSocketClients::Run(
            configs.clone(),
            peers.clone(),
            sender_to_consensus_handler.clone(),
        )
        .await;

        let mut work_done = false;
        let controller_handle = Self::run_event_loop(receiver, sender_to_consensus_handler);
        let result = tokio::join!(server_handle, controller_handle);
    }

    pub fn run_event_loop(
        mut receiver: Receiver<IoEvent>,
        sender_to_consensus_handler: Sender<ConsensusEvent>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let result = receiver.recv().await;
                if result.is_some() {
                    let event = result.unwrap();
                    let event_id = event.event_id;
                    let interface_event = event.event;

                    match interface_event {
                        // NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                        //     // trace!("waiting for the io controller write lock");
                        //     // let mut io_controller = network_controller.write().await;
                        //     // trace!("acquired the io controller write lock");
                        //     // io_controller.send_to_all(buffer, exceptions).await;
                        // }
                        NetworkEvent::OutgoingNetworkMessage {
                            peer_index: index,
                            buffer,
                        } => {
                            //trace!("waiting for the io_controller write lock");
                            //let mut io_controller = network_controller.write().await;
                            //trace!("acquired the io controller write lock");
                            //io_controller.send_outgoing_message(index, buffer).await;
                        }
                        // NetworkEvent::ConnectToPeer { peer_details } => {
                        //     unreachable!()
                        // }
                        // NetworkEvent::PeerConnectionResult { .. } => {
                        //     unreachable!()
                        // }
                        // NetworkEvent::PeerDisconnected { peer_index: _ } => {
                        //     unreachable!()
                        // }
                        // NetworkEvent::IncomingNetworkMessage { .. } => {
                        //     unreachable!()
                        // }
                        NetworkEvent::BlockFetchRequest {
                            block_hash,
                            peer_index,
                            url,
                            request_id,
                        } => {
                            // starting new thread to stop io controller from getting blocked
                            let sender = sender_to_consensus_handler.clone();
                            tokio::spawn(async move {
                                NetworkHandler::fetch_block(
                                    block_hash,
                                    peer_index,
                                    url,
                                    event_id,
                                    sender.clone(),
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

    pub async fn fetch_block(
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
        request_id: u64,
        sender_to_core: Sender<ConsensusEvent>,
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

        sender_to_core
            .send(ConsensusEvent::BlockFetched { peer_index, buffer })
            .await
            .unwrap();
        debug!("block buffer sent to blockchain controller");
    }
}

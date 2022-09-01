use std::collections::HashMap;

use std::io::{Error, ErrorKind};

use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use tokio::net::TcpStream;

use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::context::Context;
use saito_core::core::data::crypto::generate_random_bytes;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_event_processor::MiningEvent;
use saito_core::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

use crate::saito::rust_io_handler::RustIOHandler;
use crate::{IoEvent, NetworkEvent};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct NetworkConnections {
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Wallet>>,
    configuration: Arc<RwLock<Configuration>>,
    network: Arc<Mutex<Network>>,
    storage: Arc<Mutex<Storage>>,
    peers: Arc<RwLock<PeerCollection>>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
    sender_to_miner: Sender<MiningEvent>,
    spam_generators: Arc<Mutex<HashMap<u64, JoinHandle<()>>>>,
}

impl NetworkConnections {
    pub fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Configuration>>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_self: Sender<IoEvent>,
        sender_to_miner: Sender<MiningEvent>,
    ) -> Self {
        NetworkConnections {
            blockchain,
            wallet: wallet.clone(),
            configuration,
            network: Arc::new(Mutex::new(Network::new(
                Box::new(RustIOHandler::new(sender_to_self.clone(), 0)),
                peers.clone(),
                wallet.clone(),
            ))),
            storage: Arc::new(Mutex::new(Storage::new(Box::new(RustIOHandler::new(
                sender_to_self,
                0,
            ))))),
            peers,
            peer_counter: Arc::new(Mutex::new(PeerCounter { counter: 0 })),
            senders: Arc::new(Mutex::new(HashMap::new())),
            sender_to_miner,
            spam_generators: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(&mut self) {
        let peer_configs = self.configuration.read().await.peers.clone();

        for peer_config in peer_configs {
            self.connect_to_static_peer(peer_config).await;
        }
    }

    pub async fn connect_to_static_peer(&mut self, config: PeerConfig) {
        let mut counter = self.peer_counter.lock().await;
        let next_index = counter.get_next_index();

        Self::connect_to_peer(
            self.blockchain.clone(),
            self.wallet.clone(),
            self.configuration.clone(),
            self.network.clone(),
            config,
            next_index,
            self.senders.clone(),
            self.spam_generators.clone(),
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
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Configuration>>,
        network: Arc<Mutex<Network>>,
        peer_config: PeerConfig,
        peer_index: u64,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
        spam_generators: Arc<Mutex<HashMap<u64, JoinHandle<()>>>>,
    ) {
        tokio::spawn(async move {
            let url = Self::get_connection_url(&peer_config);

            loop {
                let result = connect_async(url.clone()).await;

                if result.is_ok() {
                    info!("connected to peer : {:?}", url);

                    let result = result.unwrap();
                    let socket: WebSocketStream<MaybeTlsStream<TcpStream>> = result.0;
                    let (socket_sender, socket_receiver): (SocketSender, SocketReceiver) =
                        socket.split();

                    Self::on_new_connection(
                        blockchain.clone(),
                        wallet.clone(),
                        configuration.clone(),
                        network.clone(),
                        peer_config.clone(),
                        peer_index,
                        senders.clone(),
                        socket_receiver,
                        socket_sender,
                        spam_generators.clone(),
                    )
                    .await;
                } else {
                    warn!(
                        "failed connecting to peer : {:?}, error : {:?}, reconnecting ...",
                        url,
                        result.err()
                    );
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    pub async fn on_new_connection(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Configuration>>,
        network: Arc<Mutex<Network>>,
        config: PeerConfig,
        peer_index: u64,
        senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
        mut receiver: SocketReceiver,
        sender: SocketSender,
        spam_generators: Arc<Mutex<HashMap<u64, JoinHandle<()>>>>,
    ) {
        {
            senders.lock().await.insert(peer_index, sender);
        }

        {
            network
                .lock()
                .await
                .handle_new_peer(
                    Some(config),
                    peer_index,
                    wallet.clone(),
                    configuration.clone(),
                )
                .await;
        }

        loop {
            // TODO : Check whether this can return a buffer with multiple messages
            // bunched together, seems to be the case
            let result = receiver.next().await;
            if result.is_none() {
                continue;
            }
            let result = result.unwrap();
            if result.is_err() {
                network
                    .lock()
                    .await
                    .handle_peer_disconnect(peer_index)
                    .await;
                break;
            }
            let result = result.unwrap();
            match result {
                tungstenite::Message::Binary(buffer) => {
                    if let Ok(message) = Message::deserialize(buffer) {
                        Self::handle_input_message(
                            blockchain.clone(),
                            wallet.clone(),
                            configuration.clone(),
                            network.clone(),
                            peer_index,
                            message,
                        )
                        .await
                    }
                }
                _ => {
                    // Not handling these scenarios
                    todo!()
                }
            }
        }

        {
            //Connection has exited the loop, which means disconnected
            senders.lock().await.remove(&peer_index);

            if let Some(handle) = spam_generators.lock().await.remove(&peer_index) {
                handle.abort();
            }
        }
    }

    async fn handle_input_message(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Configuration>>,
        network: Arc<Mutex<Network>>,
        peer_index: u64,
        message: Message,
    ) {
        match message {
            Message::HandshakeChallenge(handshake_challenge) => {
                network
                    .lock()
                    .await
                    .handle_handshake_challenge(
                        peer_index,
                        handshake_challenge,
                        wallet,
                        configuration,
                    )
                    .await;
            }
            Message::HandshakeResponse(handshake_response) => {
                network
                    .lock()
                    .await
                    .handle_handshake_response(peer_index, handshake_response, wallet, blockchain)
                    .await;
            }
            Message::HandshakeCompletion(handshake_completion) => {
                network
                    .lock()
                    .await
                    .handle_handshake_completion(peer_index, handshake_completion, blockchain)
                    .await;
            }
            Message::ApplicationMessage(_) => {}
            Message::Block(_) => {}
            Message::Transaction(_) => {}
            Message::BlockchainRequest(_) => {}
            Message::BlockHeaderHash(header_hash) => {
                network
                    .lock()
                    .await
                    .process_incoming_block_hash(header_hash, peer_index, blockchain)
                    .await;
            }
            _ => {}
        }
    }

    async fn send(
        &self,
        peer_index: u64,
        buffer: Vec<u8>,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        return Self::send2(&self.senders, peer_index, buffer).await;
    }

    async fn send2(
        senders: &Arc<Mutex<HashMap<u64, SocketSender>>>,
        peer_index: u64,
        buffer: Vec<u8>,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        let mut senders = senders.lock().await;
        if let Some(sender) = senders.get_mut(&peer_index) {
            // TODO : Check whether we can use the method feed here
            let result = sender.send(tungstenite::Message::Binary(buffer)).await;

            if result.is_err() {
                return Err(result.err().unwrap());
            }

            return sender.flush().await;
        }

        Ok(())
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

    pub async fn fetch_block(url: String) -> Result<Block, Error> {
        debug!("fetching block : {:?}", url);

        let result = reqwest::get(url).await;
        if result.is_err() {
            error!("Block fetching error : {:?}", result.err().unwrap());
            return Err(Error::from(ErrorKind::ConnectionRefused));
        }
        let response = result.unwrap();
        let result = response.bytes().await;
        if result.is_err() {
            error!(
                "Error converting fetched blocked into binary : {:?}",
                result.err().unwrap()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }
        let result = result.unwrap();
        let buffer = result.to_vec();
        debug!("block buffer received");

        let block = Block::deserialize_from_net(&buffer);

        Ok(block)
    }

    async fn on_block(&mut self, peer_index: u64, _block: Block) {
        let balance;
        let has_generator;
        let public_key;
        {
            log_write_lock_request!("blockchain");
            let _blockchain = self.blockchain.write().await;
            log_write_lock_receive!("blockchain");

            let _network = self.network.lock().await;
            let _storage = self.storage.lock().await;
            // TODO  : uncomment
            // blockchain
            //     .add_block(block, &network, &mut storage, self.sender_to_miner.clone(),)
            //     .await;
            {
                log_read_lock_request!("wallet");
                let wallet = self.wallet.read().await;
                log_read_lock_receive!("wallet");
                balance = wallet.get_available_balance();
                public_key = wallet.public_key;
            }
            has_generator = self.spam_generators.lock().await.get(&peer_index).is_some();

            info!(
                "New Block Added, Wallet Balance for {:?} is {:?}",
                hex::encode(public_key),
                balance
            );
        }

        if balance > 0 && !has_generator {
            let handle = self.start_spam_generator(peer_index).await;
            {
                info!("Starting the spammer for {:?}", hex::encode(public_key));
                self.spam_generators.lock().await.insert(peer_index, handle);
            }
        } else if balance == 0 && has_generator {
            info!("Stopping the spammer of {:?}", hex::encode(public_key));

            self.spam_generators
                .lock()
                .await
                .get(&peer_index)
                .unwrap()
                .abort();
        }
    }

    async fn start_spam_generator(&mut self, peer_index: u64) -> JoinHandle<()> {
        let timer_in_milli;
        let burst_count;
        let bytes_per_tx;
        {
            let config = self.configuration.read().await;
            timer_in_milli = config.spammer.timer_in_milli;
            burst_count = config.spammer.burst_count;
            bytes_per_tx = config.spammer.bytes_per_tx;
        }

        let blockchain = self.blockchain.clone();
        let wallet = self.wallet.clone();
        let senders = self.senders.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(timer_in_milli));
            loop {
                interval.tick().await;

                for _i in 0..burst_count {
                    let transaction = Self::generate_tx(&blockchain, &wallet, bytes_per_tx).await;
                    let message = Message::Transaction(transaction);
                    let buffer = message.serialize();
                    Self::send2(&senders, peer_index, buffer).await;
                }
            }
        })
    }

    async fn generate_tx(
        blockchain: &Arc<RwLock<Blockchain>>,
        wallet: &Arc<RwLock<Wallet>>,
        bytes_per_tx: u32,
    ) -> Transaction {
        trace!("generating mock transactions");

        let public_key;
        let private_key;
        //let latest_block_id;
        {
            log_read_lock_request!("wallet");
            let wallet = wallet.read().await;
            log_read_lock_receive!("wallet");
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        {
            log_read_lock_request!("blockchain");
            let blockchain = blockchain.read().await;
            log_read_lock_receive!("blockchain");

            if blockchain.blockring.is_empty() {
                unreachable!()
            }
        }

        let mut transaction = Transaction::create(wallet.clone(), public_key, 100, 100).await;
        transaction.message = generate_random_bytes(bytes_per_tx as u64);
        transaction.generate(public_key);
        transaction.sign(private_key);
        transaction.add_hop(wallet.clone(), public_key).await;
        // transaction
        //     .add_hop(self.wallet.clone(), public_key)
        //     .await;

        return transaction;
    }

    pub async fn run(context: Context, peers: Arc<RwLock<PeerCollection>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("running network handler");
            let (sender_to_miner, _receiver_for_miner) =
                tokio::sync::mpsc::channel::<MiningEvent>(1000);
            let (sender_to_self, mut receiver) = tokio::sync::mpsc::channel::<IoEvent>(1000);

            let network_connections = Arc::new(Mutex::new(NetworkConnections::new(
                context.blockchain.clone(),
                context.wallet.clone(),
                context.configuration.clone(),
                peers.clone(),
                sender_to_self,
                sender_to_miner,
            )));

            {
                network_connections.lock().await.connect().await;
            }

            loop {
                let result = receiver.recv().await;
                if result.is_some() {
                    let event = result.unwrap();
                    let interface_event = event.event;
                    match interface_event {
                        NetworkEvent::OutgoingNetworkMessageForAll { buffer, exceptions } => {
                            network_connections
                                .lock()
                                .await
                                .send_to_all(buffer, exceptions)
                                .await;
                        }
                        NetworkEvent::OutgoingNetworkMessage {
                            peer_index: index,
                            buffer,
                        } => {
                            network_connections.lock().await.send(index, buffer).await;
                        }
                        NetworkEvent::ConnectToPeer { .. } => {}
                        NetworkEvent::PeerConnectionResult { .. } => {
                            unreachable!()
                        }
                        NetworkEvent::PeerDisconnected { .. } => {
                            unreachable!()
                        }
                        NetworkEvent::IncomingNetworkMessage { .. } => {
                            unreachable!()
                        }
                        NetworkEvent::BlockFetchRequest {
                            block_hash: _,
                            peer_index,
                            url,
                        } => {
                            // starting new thread to stop io controller from getting blocked
                            let clone = network_connections.clone();

                            tokio::spawn(async move {
                                if let Ok(block) = NetworkConnections::fetch_block(url).await {
                                    clone.lock().await.on_block(peer_index, block).await;
                                }
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

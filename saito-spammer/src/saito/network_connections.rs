use std::collections::{HashMap, LinkedList};

use std::io::{Error, ErrorKind};

use std::sync::Arc;
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, trace, warn};

use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::context::Context;

use saito_core::core::data::mempool::Mempool;
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
use crate::saito::transaction_generator::TransactionGenerator;
use crate::{IoEvent, NetworkEvent, SpammerConfiguration};

type SocketSender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
type SocketReceiver = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct NetworkConnections {
    blockchain: Arc<RwLock<Blockchain>>,
    wallet: Arc<RwLock<Wallet>>,
    configuration: Arc<RwLock<Box<SpammerConfiguration>>>,
    core_configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
    mempool: Arc<RwLock<Mempool>>,
    network: Arc<Mutex<Network>>,
    storage: Arc<Mutex<Storage>>,
    peers: Arc<RwLock<PeerCollection>>,
    peer_counter: Arc<Mutex<PeerCounter>>,
    senders: Arc<Mutex<HashMap<u64, SocketSender>>>,
    sender_to_miner: Sender<MiningEvent>,
    spam_generators: Arc<Mutex<HashMap<u64, JoinHandle<()>>>>,
    transaction_generator: TransactionGenerator,
}

impl NetworkConnections {
    pub async fn new(
        blockchain: Arc<RwLock<Blockchain>>,
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Box<SpammerConfiguration>>>,
        core_configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
        mempool: Arc<RwLock<Mempool>>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_self: Sender<IoEvent>,
        sender_to_miner: Sender<MiningEvent>,
    ) -> Self {
        NetworkConnections {
            blockchain,
            wallet: wallet.clone(),
            configuration: configuration.clone(),
            core_configuration,
            mempool,
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
            transaction_generator: TransactionGenerator::create(wallet.clone(), configuration)
                .await,
        }
    }

    pub async fn connect(&mut self) {
        let peer_configs = self.configuration.read().await.get_peer_configs().clone();

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
            self.core_configuration.clone(),
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
        configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
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
        configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
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
                            configuration.clone()
                                as Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
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
        configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>>,
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

    async fn on_block(&mut self, peer_index: u64, block: Block) {
        let balance;
        let public_key;
        {
            log_write_lock_request!("blockchain");
            let mut blockchain = self.blockchain.write().await;
            log_write_lock_receive!("blockchain");

            let network = self.network.lock().await;
            let mut storage = self.storage.lock().await;

            blockchain
                .add_block(
                    block,
                    &network,
                    &mut storage,
                    self.sender_to_miner.clone(),
                    self.mempool.clone(),
                )
                .await;

            {
                log_read_lock_request!("wallet");
                let wallet = self.wallet.read().await;
                log_read_lock_receive!("wallet");
                balance = wallet.get_available_balance();
                let unspent_slips = wallet.get_unspent_slip_count();
                public_key = wallet.public_key;

                info!(
                    "New Block Added, Wallet Balance for {:?} is {:?}, unspent slips {:?}",
                    hex::encode(public_key),
                    balance,
                    unspent_slips,
                );
            }
        }

        let transactions = self.transaction_generator.on_new_block().await;

        if let Some(transactions) = transactions {
            let timer_in_milli;
            let burst_count;
            {
                let config = self.configuration.read().await;
                timer_in_milli = config.get_spammer_configs().timer_in_milli;
                burst_count = config.get_spammer_configs().burst_count;
            }
            self.send_transactions(peer_index, transactions, timer_in_milli, burst_count)
                .await;
        }
    }

    async fn send_transactions(
        &mut self,
        peer_index: u64,
        mut transactions: LinkedList<Transaction>,
        timer_in_milli: u64,
        burst_count: u32,
    ) {
        let tx_count = transactions.len();
        info!("Transaction sending started, tx count = {:?}, timer interval = {:?}, burst count {:?}, starting thread ...", tx_count, timer_in_milli, burst_count);

        let senders = self.senders.clone();
        let spam_generators = self.spam_generators.clone();

        let handle = tokio::spawn(async move {
            let mut sent_count: u64 = 0;
            let mut interval = interval(Duration::from_millis(timer_in_milli));
            let mut send_complete = false;
            loop {
                interval.tick().await;

                for _i in 0..burst_count {
                    if let Some(transaction) = transactions.pop_front() {
                        sent_count += 1;
                        trace!("Sending transaction {:?} of {:?}", sent_count, tx_count);

                        let message = Message::Transaction(transaction);
                        let buffer = message.serialize();
                        Self::send2(&senders, peer_index, buffer).await;
                    } else {
                        info!("Transaction sending completed, a total of {:?} transactions sent, exiting loop ...", sent_count);
                        send_complete = true;
                        break;
                    }
                }

                if send_complete {
                    break;
                }
            }

            spam_generators.lock().await.remove(&peer_index);
        });

        self.spam_generators.lock().await.insert(peer_index, handle);
    }

    pub async fn run(
        context: Context,
        peers: Arc<RwLock<PeerCollection>>,
        configs: Arc<RwLock<Box<SpammerConfiguration>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("running network handler");
            let (sender_to_miner, _receiver_for_miner) =
                tokio::sync::mpsc::channel::<MiningEvent>(1000);
            let (sender_to_self, mut receiver) = tokio::sync::mpsc::channel::<IoEvent>(1000);

            let network_connections = Arc::new(Mutex::new(
                NetworkConnections::new(
                    context.blockchain.clone(),
                    context.wallet.clone(),
                    configs,
                    context.configuration.clone(),
                    context.mempool.clone(),
                    peers.clone(),
                    sender_to_self,
                    sender_to_miner,
                )
                .await,
            ));

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

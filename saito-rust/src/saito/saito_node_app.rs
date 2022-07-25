use crate::saito::config_handler::ConfigHandler;
use crate::saito::mempool_handler::{MempoolEvent, MempoolHandler};
use crate::saito::mining_handler::MiningHandler;
use crate::saito::web_socket_clients::WebSocketClients;
use crate::saito::web_socket_server::WebSocketServer;
use log::info;
use saito_core::core::data::context::Context;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

pub struct SaitoNodeApp {}

impl SaitoNodeApp {
    pub async fn run(config_file_pathname: String) {
        let timestamp_at_start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        tracing_subscriber::fmt::init();

        info!(
            "Starting Saito Node, Current Directory : {:?}",
            std::env::current_dir().unwrap()
        );

        let configuration = Arc::new(RwLock::new(
            ConfigHandler::load_configs(config_file_pathname).expect("loading configs failed"),
        ));

        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel::<MiningEvent>(1000);

        let (sender_to_mempool, receiver_in_mempool) =
            tokio::sync::mpsc::channel::<MempoolEvent>(1000);

        info!("Starting Saito handlers");

        let context = Context::new(configuration.clone());

        let peers = Arc::new(RwLock::new(PeerCollection::new(
            configuration.clone(),
            context.blockchain.clone(),
            context.mempool.clone(),
            context.wallet.clone(),
        )));

        let mempool_handler = MempoolHandler::new(
            &context,
            peers.clone(),
            sender_to_miner.clone(),
            sender_to_mempool.clone(),
            receiver_in_mempool,
        );
        let mining_handler =
            MiningHandler::new(&context, receiver_in_miner, sender_to_mempool.clone());
        let ws_clients = WebSocketClients::new(&context, peers.clone(), sender_to_miner.clone());
        let ws_server = WebSocketServer::new(&context, peers.clone(), sender_to_miner.clone());

        let mempool_handle = MempoolHandler::run(mempool_handler).await;
        let miner_handle = MiningHandler::run(mining_handler).await;
        ws_clients.connect().await;
        let web_server_handle = ws_server.run().await;

        let timestamp_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let elapsed_time = (timestamp_now as f64 - timestamp_at_start as f64) / 1000.0 as f64;

        info!(
            "Saito Node has Started, Elapsed Time {:?} seconds",
            elapsed_time
        );

        let _result = tokio::join!(mempool_handle, miner_handle, web_server_handle);
    }
}

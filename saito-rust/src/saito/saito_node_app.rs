use crate::saito::consensus_handler::{ConsensusEvent, ConsensusHandler};
use crate::saito::mining_handler::MiningHandler;
use crate::saito::network_handler::NetworkHandler;
use crate::{ConfigHandler, IoEvent, RustIOHandler};
use log::info;
use saito_core::core::data::context::Context;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::mining_event_processor::MiningEvent;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SaitoNodeApp {}

impl SaitoNodeApp {
    pub async fn run() {
        tracing_subscriber::fmt::init();

        let configs = Arc::new(RwLock::new(
            ConfigHandler::load_configs("configs/saito.config.json".to_string())
                .expect("loading configs failed"),
        ));

        let (sender_to_mining_handler, receiver_in_mining_handler) =
            tokio::sync::mpsc::channel::<MiningEvent>(1000);

        let (sender_to_consensus_handler, receiver_consensus_handler) =
            tokio::sync::mpsc::channel::<ConsensusEvent>(1000);

        let (sender_to_network_handler, receiver_in_network_handler) =
            tokio::sync::mpsc::channel::<IoEvent>(1000);

        info!("running saito controllers");

        let context = Context::new(configs.clone());
        let peers = Arc::new(RwLock::new(PeerCollection::new(
            configs.clone(),
            context.blockchain.clone(),
            context.wallet.clone(),
        )));

        let consensus_handler = ConsensusHandler {
            mempool: context.mempool.clone(),
            blockchain: context.blockchain.clone(),
            wallet: context.wallet.clone(),
            sender_to_miner: sender_to_mining_handler.clone(),
            network: Network::new(
                Box::new(RustIOHandler::new(sender_to_network_handler.clone(), 1)),
                peers.clone(),
                context.blockchain.clone(),
            ),
            storage: Storage::new(Box::new(RustIOHandler::new(
                sender_to_network_handler.clone(),
                1,
            ))),
        };

        let mining_handler = MiningHandler {
            wallet: context.wallet.clone(),
            sender_to_consensus_handler: sender_to_consensus_handler.clone(),
        };

        ConsensusHandler::run(
            consensus_handler,
            sender_to_consensus_handler.clone(),
            receiver_consensus_handler,
        );

        MiningHandler::run(mining_handler, receiver_in_mining_handler);

        NetworkHandler::run(
            receiver_in_network_handler,
            sender_to_consensus_handler.clone(),
            context.configuration.clone(),
            context.blockchain.clone(),
            context.wallet.clone(),
            peers.clone(),
        );
    }
}

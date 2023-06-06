use std::collections::VecDeque;
use std::panic;
use std::process;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::info;
use log::{debug, error, trace};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing_subscriber;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::{push_lock, StatVariable, LOCK_ORDER_CONFIGS, STAT_BIN_COUNT};
use saito_core::core::consensus_thread::{ConsensusEvent};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::routing_thread::{
    PeerState, RoutingEvent, RoutingStats, RoutingThread, StaticPeer,
};
use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};
use saito_core::lock_for_read;
use saito_rust::saito::config_handler::ConfigHandler;
use saito_rust::saito::io_event::IoEvent;
use saito_rust::saito::network_controller::run_network_controller;
use saito_rust::saito::rust_io_handler::RustIOHandler;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

async fn run_utxodump(
    context: &Context,
    peers: Arc<RwLock<PeerCollection>>,
    sender_to_network_controller: Sender<IoEvent>,
) {
    debug!("run_utxodump");

    let mut store = Storage::new(Box::new(RustIOHandler::new(
        sender_to_network_controller.clone(),
        CONSENSUS_EVENT_PROCESSOR_ID,
    )));

    debug!(".......");
    debug!(">> {:?}", store);
    // TODO want to pass in the directory
    // loading blocks from dir : "./data/blocks/"

    let result = store.load_blocks_from_disk_vec().await;

    match result {
        Ok(blocks) => {
            debug!(">>  id: {:?}", blocks[0].id);
            for block in &blocks {
                println!("{:?}", block.id);
            }
        }
        Err(e) => {
            // Handle error case
            eprintln!("Error loading blocks: {}", e);
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(move || {
        info!("shutting down the node");
        process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        if let Some(location) = panic_info.location() {
            error!(
                "panic occurred in file '{}' at line {}, exiting ..",
                location.file(),
                location.line()
            );
        } else {
            error!("panic occurred but can't get location information, exiting ..");
        }

        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(99);
    }));

    println!("Running saito analytics");

    let filter = tracing_subscriber::EnvFilter::from_default_env();
    let filter = filter.add_directive(Directive::from_str("tokio_tungstenite=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("tungstenite=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("mio::poll=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("hyper::proto=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("hyper::client=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("want=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("reqwest::async_impl=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("reqwest::connect=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("warp::filters=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("saito_stats=info").unwrap());

    let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);

    tracing_subscriber::registry().with(fmt_layer).init();

    let configs: Arc<RwLock<dyn Configuration + Send + Sync>> = Arc::new(RwLock::new(
        ConfigHandler::load_configs("configs/config.json".to_string())
            .expect("loading configs failed"),
    ));

    let channel_size;
    let thread_sleep_time_in_ms;
    let stat_timer_in_ms;
    let verification_thread_count;
    let fetch_batch_size;

    {
        let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);

        channel_size = configs.get_server_configs().unwrap().channel_size as usize;
        thread_sleep_time_in_ms = configs
            .get_server_configs()
            .unwrap()
            .thread_sleep_time_in_ms;
        stat_timer_in_ms = configs.get_server_configs().unwrap().stat_timer_in_ms;
        verification_thread_count = configs.get_server_configs().unwrap().verification_threads;
        fetch_batch_size = configs.get_server_configs().unwrap().block_fetch_batch_size as usize;
        assert_ne!(fetch_batch_size, 0);
    }

    let (event_sender_to_loop, event_receiver_in_loop) =
        tokio::sync::mpsc::channel::<IoEvent>(channel_size);

    let (sender_to_network_controller, receiver_in_network_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(channel_size);

    let keys = generate_keys();
    let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
    {
        let mut wallet = wallet.write().await;
        Wallet::load(
            &mut wallet,
            Box::new(RustIOHandler::new(
                sender_to_network_controller.clone(),
                ROUTING_EVENT_PROCESSOR_ID,
            )),
        )
        .await;
    }
    let context = Context::new(configs.clone(), wallet);

    let peers = Arc::new(RwLock::new(PeerCollection::new()));

    let (sender_to_consensus, receiver_for_consensus) =
        tokio::sync::mpsc::channel::<ConsensusEvent>(channel_size);

    let (sender_to_routing, receiver_for_routing) =
        tokio::sync::mpsc::channel::<RoutingEvent>(channel_size);

    //let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));

    run_utxodump(&context, peers.clone(), sender_to_network_controller).await;

    Ok(())
}

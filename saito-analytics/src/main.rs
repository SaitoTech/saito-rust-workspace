use std::collections::VecDeque;
use std::panic;
use std::process;
use std::str::FromStr;
use std::sync::Arc;
use ahash::AHashMap;

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
use saito_core::common::defs::{push_lock, LOCK_ORDER_CONFIGS};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::network::Network;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::data::context::Context;

use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};
use saito_core::lock_for_read;
use saito_core::lock_for_write;
//TODO from saito rust not analytics
use saito_rust::saito::config_handler::ConfigHandler;
use saito_rust::saito::io_event::IoEvent;
use saito_rust::saito::rust_io_handler::RustIOHandler;

use saito_core::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, Timestamp,
    UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET,
};

use saito_core::core::data::mempool::Mempool;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

async fn run_utxodump(        
    store: Storage,
    wallet: Arc<RwLock<Wallet>>,
    context: &Context,
    //mempool: Arc<RwLock<Mempool>>
) {

    

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

    let channel_size = 1000;

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
    debug!("run analytics.....");

    let wallet_clone = Arc::clone(&wallet);

    let context = Context::new(configs.clone(), wallet_clone);

    let mut store = Storage::new(Box::new(RustIOHandler::new(
        sender_to_network_controller.clone(),
        CONSENSUS_EVENT_PROCESSOR_ID,
    )));

    //TODO in fn

    info!("........run_utxodump");

    info!("{:?} ", context.mempool);
    

    //run_utxodump(sender_to_network_controller, wallet, &context).await;
    
    store.load_blocks_from_disk(context.mempool).await;

    //debug!("{:?} ", context.mempool.);

    //let wallet_lock = wallet.clone();
    let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet.clone())));

    let (mut blockchain, _blockchain_) = lock_for_write!(blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
    info!("genesis_block_id: {:?}", blockchain.genesis_block_id);
    info!("block last_block_id: {:?}", blockchain.last_block_id);
    info!("last_timestamp: {:?}", blockchain.last_timestamp);

    let mut utxoset: UtxoSet = AHashMap::new();
    let latest_block_id = blockchain.get_latest_block_id();

    info!("---- check utxoset ");
    for i in 0..=latest_block_id {
        info!(".... {}", i);
        let block_hash = blockchain
            .blockring
            .get_longest_chain_block_hash_by_block_id(i as u64);
        info!("block_hash {:?}", block_hash);

        // let previous_block_hash = blockchain
        //             .blockring
        //             .get_longest_chain_block_hash_by_block_id((i as u64) - 1);

        //let block = blockchain.get_block_sync(&block_hash);
        //let previous_block = blockchain.get_block_sync(&previous_block_hash);


        // info!("WINDING ID HASH - {} {:?}", i, block_hash);
        // let block = blockchain.get_block(&block_hash).unwrap();
        // for j in 0..block.transactions.len() {
        //     block.transactions[j].on_chain_reorganization(&mut utxoset, true, i as u64);
        // }
    }

    Ok(())
}



//     info!(">> {:?}", store);
//     // TODO want to pass in the directory
//     // loading blocks from dir : "./data/blocks/"

//     //let blocks = store.load_blocks_from_disk_vec().await.unwrap();
//let blocks = store.load_blocks_from_disk().await.unwrap();

//     // info!(">>  id: {:?}", blocks[0].id);
//     // for block in blocks {
//     //     info!("{:?}", block.id);
//     // }


    
//     // blockchain.add_block_tmp(blocks[0]);    
    
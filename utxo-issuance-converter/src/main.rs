use std::ops::Deref;
use std::path::Path;
use std::process;
use std::sync::Arc;

use log::{error, info};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing_subscriber;
use tracing_subscriber::util::SubscriberInitExt;

use saito_core::common::defs::{
    push_lock, Currency, SaitoPrivateKey, SaitoPublicKey, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
};
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::MiningEvent;
use saito_core::{lock_for_read, lock_for_write};
use saito_rust::saito::config_handler::NodeConfigurations;
use saito_rust::saito::io_event::IoEvent;
use saito_rust::saito::rust_io_handler::RustIOHandler;

// use crate::saito::rust_io_handler::RustIOHandler;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(move || {
        info!("shutting down the node");
        process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // let orig_hook = panic::take_hook();
    // panic::set_hook(Box::new(move |panic_info| {
    //     if let Some(location) = panic_info.location() {
    //         error!(
    //             "panic occurred in file '{}' at line {}, exiting ..",
    //             location.file(),
    //             location.line()
    //         );
    //     } else {
    //         error!("panic occurred but can't get location information, exiting ..");
    //     }
    //
    //     // invoke the default handler and exit the process
    //     orig_hook(panic_info);
    //     process::exit(99);
    // }));
    //
    // println!("Running saito");
    //
    // // install global subscriber configured based on RUST_LOG envvar.
    //
    // let filter = tracing_subscriber::EnvFilter::from_default_env();
    // let filter = filter.add_directive(Directive::from_str("tokio_tungstenite=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("tungstenite=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("mio::poll=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("hyper::proto=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("hyper::client=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("want=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("reqwest::async_impl=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("reqwest::connect=info").unwrap());
    // let filter = filter.add_directive(Directive::from_str("warp::filters=info").unwrap());
    // // let filter = filter.add_directive(Directive::from_str("saito_stats=info").unwrap());
    //
    // let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);
    //
    // tracing_subscriber::registry().with(fmt_layer).init();
    // tracing_subscriber::registry().init();

    env_logger::init();

    let public_key: SaitoPublicKey =
        hex::decode("03145c7e7644ab277482ba8801a515b8f1b62bcd7e4834a33258f438cd7e223849")
            .unwrap()
            .try_into()
            .unwrap();
    let private_key: SaitoPrivateKey =
        hex::decode("ddb4ba7e5d70c2234f035853902c6bc805cae9163085f2eac5e585e2d6113ccd")
            .unwrap()
            .try_into()
            .unwrap();

    println!("Public Key : {:?}", hex::encode(public_key));
    println!("Private Key : {:?}", hex::encode(private_key));

    let configs_lock: Arc<RwLock<NodeConfigurations>> =
        Arc::new(RwLock::new(NodeConfigurations::default()));

    let configs_clone: Arc<RwLock<dyn Configuration + Send + Sync>> = configs_lock.clone();

    let (sender_to_network_controller, receiver_in_network_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(100);

    info!("running saito controllers");

    let wallet = Arc::new(RwLock::new(Wallet::new(private_key, public_key)));
    {
        let mut wallet = wallet.write().await;
        let (sender, _receiver) = tokio::sync::mpsc::channel::<IoEvent>(100);
        Wallet::load(&mut wallet, Box::new(RustIOHandler::new(sender, 1))).await;
    }
    let context = Context::new(configs_clone.clone(), wallet);

    let mut storage = Storage::new(Box::new(RustIOHandler::new(
        sender_to_network_controller.clone(),
        0,
    )));
    storage.load_blocks_from_disk(context.mempool.clone()).await;

    let peers_lock = Arc::new(RwLock::new(PeerCollection::new()));

    let (sender_to_miner, receiver_for_miner) = tokio::sync::mpsc::channel::<MiningEvent>(100);

    let (configs, _configs_) = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);

    let (mut blockchain, _blockchain_) = lock_for_write!(context.blockchain, LOCK_ORDER_BLOCKCHAIN);
    blockchain
        .add_blocks_from_mempool(
            context.mempool.clone(),
            None,
            &mut storage,
            sender_to_miner.clone(),
            configs.deref(),
        )
        .await;

    let data = blockchain.get_utxoset_data();

    info!("{:?} entries to write to file", data.len());
    let issuance_path: String = "./data/issuance".to_string();
    let threshold: Currency = 0;
    info!("opening file : {:?}", issuance_path);

    let path = Path::new(issuance_path.as_str());
    if path.parent().is_some() {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .expect("failed creating directory structure");
    }

    let file = File::create(issuance_path.clone()).await;
    if file.is_err() {
        error!("error opening file. {:?}", file.err().unwrap());
        File::create(issuance_path)
            .await
            .expect("couldn't create file");
        return Ok(());
    }
    let mut file = file.unwrap();

    let txtype = "Normal";

    for (key, value) in &data {
        if value > &threshold {
            let key_base58 = bs58::encode(key).into_string();
            file.write_all(format!("{}\t{}\t{}\n", value, key_base58, txtype).as_bytes())
                .await
                .expect("failed writing to issuance file");
        }
    }
    file.flush()
        .await
        .expect("failed flushing issuance file data");

    Ok(())
}

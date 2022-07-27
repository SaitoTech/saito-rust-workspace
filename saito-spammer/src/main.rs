use std::panic;
use std::process;
use std::sync::Arc;

use log::error;
use tokio::sync::RwLock;
use tracing_subscriber;

use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey};
use saito_core::core::data::context::Context;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;

use crate::saito::config_handler::ConfigHandler;
use crate::saito::io_event::IoEvent;
use crate::saito::network_connections::NetworkConnections;
use crate::saito::rust_io_handler::RustIOHandler;

mod saito;
mod test;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    println!("Running saito");

    tracing_subscriber::fmt::init();

    let configs = Arc::new(RwLock::new(
        ConfigHandler::load_configs("configs/saito.config.json".to_string())
            .expect("loading configs failed"),
    ));

    let context = Context::new(configs.clone());

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
    {
        let mut wallet = context.wallet.write().await;
        wallet.private_key = private_key;
        wallet.public_key = public_key;

        let (sender, mut receiver) = tokio::sync::mpsc::channel::<IoEvent>(1000);
        let mut storage = Storage::new(Box::new(RustIOHandler::new(sender, 1)));
        wallet.load(&mut storage).await;
    }

    let peers = Arc::new(RwLock::new(PeerCollection::new()));
    let network_handle = NetworkConnections::run(context.clone(), peers.clone()).await;

    let _result = tokio::join!(network_handle);
    Ok(())
}

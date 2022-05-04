use std::env;
use std::io::Write;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber;

use saito_core::common::command::InterfaceEvent;

use crate::saito::config_handler::ConfigHandler;
use crate::saito::io_controller::run_io_controller;
use crate::saito::io_event::IoEvent;

use std::time::{Duration, Instant};

use log::{debug, trace};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use saito_core::common::command::GlobalEvent;
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::blockchain_controller::{
    BlockchainController, BlockchainEvent, PeerState, StaticPeer,
};
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::mempool_controller::{MempoolController, MempoolEvent};
use saito_core::core::miner_controller::{MinerController, MinerEvent};

use crate::saito::rust_io_handler::RustIOHandler;
use crate::saito::time_keeper::TimeKeeper;

mod saito;
mod test;

const BLOCKCHAIN_CONTROLLER_ID: u8 = 1;
const MEMPOOL_CONTROLLER_ID: u8 = 2;
const MINER_CONTROLLER_ID: u8 = 3;

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
    mut global_receiver: tokio::sync::broadcast::Receiver<GlobalEvent>,
    mut interface_event_receiver: Receiver<InterfaceEvent>,
    mut event_receiver: Receiver<T>,
) -> JoinHandle<()>
where
    T: Send + 'static,
{
    tokio::spawn(async move {
        info!("new thread started");
        let mut work_done = false;
        let mut last_timestamp = Instant::now();

        event_processor.on_init().await;

        loop {
            // TODO : refactor to support async calls
            let result = global_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_global_event(event).await.is_some() {
                    work_done = true;
                }
            }

            let result = interface_event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor
                    .process_interface_event(event)
                    .await
                    .is_some()
                {
                    work_done = true;
                }
            }

            let result = event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_event(event).await.is_some() {
                    work_done = true;
                }
            }

            let current_instant = Instant::now();
            let duration = current_instant.duration_since(last_timestamp);
            last_timestamp = current_instant;

            if event_processor
                .process_timer_event(duration)
                .await
                .is_some()
            {
                work_done = true;
            }

            if work_done {
                work_done = false;
                // std::thread::yield_now();
                tokio::task::yield_now().await;
            } else {
                std::thread::sleep(Duration::new(0, 1000_000));
            }
        }
    })
}

async fn run_miner_controller(
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    sender_to_mempool: &tokio::sync::mpsc::Sender<MempoolEvent>,
    sender_to_blockchain: &tokio::sync::mpsc::Sender<BlockchainEvent>,
    receiver_for_miner: tokio::sync::mpsc::Receiver<MinerEvent>,
) -> (Sender<InterfaceEvent>, JoinHandle<()>) {
    let miner_controller = MinerController {
        miner: context.miner.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        time_keeper: Box::new(TimeKeeper {}),
    };
    let (interface_sender_to_miner, interface_receiver_for_miner) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);

    debug!("running miner thread");
    let _miner_handle = run_thread(
        Box::new(miner_controller),
        global_sender.subscribe(),
        interface_receiver_for_miner,
        receiver_for_miner,
    )
    .await;
    (interface_sender_to_miner, _miner_handle)
}

async fn run_mempool_controller(
    sender_to_io_controller: tokio::sync::mpsc::Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    receiver_for_mempool: Receiver<MempoolEvent>,
    sender_to_blockchain: &Sender<BlockchainEvent>,
    sender_to_miner: tokio::sync::mpsc::Sender<MinerEvent>,
) -> (Sender<InterfaceEvent>, JoinHandle<()>) {
    let result = std::env::var("GEN_TX");
    let mut generate_test_tx = false;
    if result.is_ok() {
        generate_test_tx = result.unwrap().eq("1");
    }
    let mut mempool_controller = MempoolController {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        wallet: context.wallet.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_miner: sender_to_miner.clone(),
        // sender_global: global_sender.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        block_producing_timer: 0,
        tx_producing_timer: 0,
        generate_test_tx,
        io_handler: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            MEMPOOL_CONTROLLER_ID,
        )),
        peers: context.peers.clone(),
        static_peers: vec![],
    };
    {
        trace!("waiting for the configs write lock");
        let configs = configs.read().await;
        trace!("acquired the configs write lock");
        let peers = &configs.peers;
        for peer in peers {
            mempool_controller.static_peers.push(StaticPeer {
                peer_details: (*peer).clone(),
                peer_state: PeerState::Disconnected,
                peer_index: 0,
            });
        }
    }

    let (interface_sender_to_mempool, interface_receiver_for_mempool) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);
    debug!("running mempool thread");
    let _mempool_handle = run_thread(
        Box::new(mempool_controller),
        global_sender.subscribe(),
        interface_receiver_for_mempool,
        receiver_for_mempool,
    )
    .await;

    (interface_sender_to_mempool, _mempool_handle)
}

async fn run_blockchain_controller(
    sender_to_io_controller: tokio::sync::mpsc::Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    sender_to_mempool: &Sender<MempoolEvent>,
    receiver_for_blockchain: Receiver<BlockchainEvent>,
    sender_to_miner: &Sender<MinerEvent>,
) -> (Sender<InterfaceEvent>, JoinHandle<()>) {
    let mut blockchain_controller = BlockchainController {
        blockchain: context.blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        sender_to_miner: sender_to_miner.clone(),
        io_handler: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            BLOCKCHAIN_CONTROLLER_ID,
        )),
        time_keeper: Box::new(TimeKeeper {}),
        peers: context.peers.clone(),
        static_peers: vec![],
        configs: configs.clone(),
        wallet: context.wallet.clone(),
    };
    {
        trace!("waiting for the configs write lock");
        let configs = configs.read().await;
        trace!("acquired the configs write lock");
        let peers = &configs.peers;
        for peer in peers {
            blockchain_controller.static_peers.push(StaticPeer {
                peer_details: (*peer).clone(),
                peer_state: PeerState::Disconnected,
                peer_index: 0,
            });
        }
    }

    let (interface_sender_to_blockchain, interface_receiver_for_blockchain) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);

    debug!("running blockchain thread");
    let _blockchain_handle = run_thread(
        Box::new(blockchain_controller),
        global_sender.subscribe(),
        interface_receiver_for_blockchain,
        receiver_for_blockchain,
    )
    .await;

    (interface_sender_to_blockchain, _blockchain_handle)
}

fn run_loop_thread(
    mut receiver: Receiver<IoEvent>,
    interface_sender_to_blockchain: Sender<InterfaceEvent>,
    interface_sender_to_mempool: Sender<InterfaceEvent>,
    interface_sender_to_miner: Sender<InterfaceEvent>,
) -> JoinHandle<()> {


    let loop_handle = tokio::spawn(async move {
        let mut work_done: bool;
        loop {
            work_done = false;

            let result = receiver.try_recv();
            if result.is_ok() {
                let command = result.unwrap();
                // TODO : remove hard coded values
                match command.controller_id {
                    BLOCKCHAIN_CONTROLLER_ID => {
                        interface_sender_to_blockchain
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    MEMPOOL_CONTROLLER_ID => {
                        interface_sender_to_mempool
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    MINER_CONTROLLER_ID => {
                        interface_sender_to_miner.send(command.event).await.unwrap();
                    }
                    _ => {
                        unreachable!()
                    }
                }
            }

            if !work_done {
                std::thread::sleep(Duration::new(1, 0));
            }
        }
    });

    loop_handle
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running saito");

    // pretty_env_logger::init();
    // let mut builder = pretty_env_logger::formatted_builder();
    // builder
    //     .format(|buf, record| {
    //         let mut style = buf.style();
    //
    //         // TODO : set colored output
    //         style.set_bold(true);
    //         writeln!(
    //             buf,
    //             "{:6} {:2?} - {:45}- {:?}",
    //             style.value(record.level()),
    //             // record.level(),
    //             std::thread::current().id(),
    //             record.module_path().unwrap_or_default(),
    //             record.args(),
    //         )
    //     })
    //     .parse_filters(&env::var("RUST_LOG").unwrap_or_default())
    //     .init();

    // install global subscriber configured based on RUST_LOG envvar.
    tracing_subscriber::fmt::init();

    let configs = Arc::new(RwLock::new(
        ConfigHandler::load_configs("configs/saito.config.json".to_string())
            .expect("loading configs failed"),
    ));

    let (sender_to_saito_controller, receiver_in_saito_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    let (sender_to_io_controller, receiver_in_io_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    info!("running saito controllers");

    let (global_sender, global_receiver) = tokio::sync::broadcast::channel::<GlobalEvent>(1000);

    let context = Context::new(configs.clone(), global_sender.clone());

    let (sender_to_mempool, receiver_for_mempool) =
        tokio::sync::mpsc::channel::<MempoolEvent>(1000);

    let (sender_to_blockchain, receiver_for_blockchain) =
        tokio::sync::mpsc::channel::<BlockchainEvent>(1000);

    let (sender_to_miner, receiver_for_miner) = tokio::sync::mpsc::channel::<MinerEvent>(1000);

    let (interface_sender_to_blockchain, blockchain_handle) = run_blockchain_controller(
        sender_to_io_controller.clone(),
        configs.clone(),
        &global_sender,
        &context,
        &sender_to_mempool,
        receiver_for_blockchain,
        &sender_to_miner,
    )
    .await;

    let (interface_sender_to_mempool, mempool_handle) = run_mempool_controller(
        sender_to_io_controller.clone(),
        configs.clone(),
        &global_sender,
        &context,
        receiver_for_mempool,
        &sender_to_blockchain,
        sender_to_miner,
    )
    .await;

    let (interface_sender_to_miner, miner_handle) = run_miner_controller(
        &global_sender,
        &context,
        &sender_to_mempool,
        &sender_to_blockchain,
        receiver_for_miner,
    )
    .await;

    let loop_handle = run_loop_thread(
        receiver_in_saito_controller,
        interface_sender_to_blockchain,
        interface_sender_to_mempool,
        interface_sender_to_miner,
    );

    let result2 = tokio::spawn(run_io_controller(
        receiver_in_io_controller,
        sender_to_saito_controller.clone(),
        configs.clone(),
        context.blockchain.clone(),
    ));

    let result = tokio::join!(
        blockchain_handle,
        mempool_handle,
        miner_handle,
        loop_handle,
        result2
    );
    Ok(())
}

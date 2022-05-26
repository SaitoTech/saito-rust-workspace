use std::env;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, trace};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use tracing_subscriber;

use saito_core::common::command::GlobalEvent;
use saito_core::common::command::NetworkEvent;
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::consensus_event_processor::{ConsensusEvent, ConsensusEventProcessor};
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::network::Network;
use saito_core::core::data::storage::Storage;
use saito_core::core::mining_event_processor::{MiningEvent, MiningEventProcessor};
use saito_core::core::routing_event_processor::{
    PeerState, RoutingEvent, RoutingEventProcessor, StaticPeer,
};

use crate::saito::config_handler::ConfigHandler;
use crate::saito::io_event::IoEvent;
use crate::saito::network_controller::run_network_controller;
use crate::saito::rust_io_handler::RustIOHandler;
use crate::saito::time_keeper::TimeKeeper;

mod saito;
mod test;

const ROUTING_CONTROLLER_ID: u8 = 1;
const BLOCKCHAIN_CONTROLLER_ID: u8 = 2;
const MINER_CONTROLLER_ID: u8 = 3;

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
    mut global_receiver: tokio::sync::broadcast::Receiver<GlobalEvent>,
    mut network_event_receiver: Receiver<NetworkEvent>,
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

            let result = network_event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_network_event(event).await.is_some() {
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
                tokio::task::yield_now().await;
                // std::thread::yield_now();
            } else {
                tokio::task::yield_now().await;
                // std::thread::sleep(Duration::new(0, 1000_000));
            }
        }
    })
}

async fn run_miner_controller(
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    sender_to_mempool: &Sender<ConsensusEvent>,
    sender_to_blockchain: &Sender<RoutingEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let miner_controller = MiningEventProcessor {
        miner: context.miner.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        miner_timer: 0,
        new_miner_event_received: false,
    };
    let (interface_sender_to_miner, interface_receiver_for_miner) =
        tokio::sync::mpsc::channel::<NetworkEvent>(1000);

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

async fn run_blockchain_controller(
    configs: Arc<RwLock<Configuration>>,
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    receiver_for_blockchain: Receiver<ConsensusEvent>,
    sender_to_routing: &Sender<RoutingEvent>,
    sender_to_miner: Sender<MiningEvent>,
    sender_to_network_controller: Sender<IoEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let result = std::env::var("GEN_TX");
    let mut generate_test_tx = false;
    if result.is_ok() {
        generate_test_tx = result.unwrap().eq("1");
    }
    let mut blockchain_controller = ConsensusEventProcessor {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        wallet: context.wallet.clone(),
        sender_to_router: sender_to_routing.clone(),
        sender_to_miner: sender_to_miner.clone(),
        // sender_global: global_sender.clone(),
        time_keeper: Box::new(TimeKeeper {}),

        network: Network {
            peers: context.peers.clone(),
            io_handler: Box::new(RustIOHandler::new(
                sender_to_network_controller.clone(),
                BLOCKCHAIN_CONTROLLER_ID,
            )),
        },
        block_producing_timer: 0,
        tx_producing_timer: 0,
        generate_test_tx,

        peers: context.peers.clone(),
        storage: Storage {
            io_handler: Box::new(RustIOHandler::new(
                sender_to_network_controller.clone(),
                BLOCKCHAIN_CONTROLLER_ID,
            )),
        },
    };

    let (interface_sender_to_blockchain, interface_receiver_for_mempool) =
        tokio::sync::mpsc::channel::<NetworkEvent>(1000);
    debug!("running mempool thread");
    let blockchain_handle = run_thread(
        Box::new(blockchain_controller),
        global_sender.subscribe(),
        interface_receiver_for_mempool,
        receiver_for_blockchain,
    )
    .await;

    (interface_sender_to_blockchain, blockchain_handle)
}

async fn run_routing_controller(
    sender_to_io_controller: Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    global_sender: &tokio::sync::broadcast::Sender<GlobalEvent>,
    context: &Context,
    sender_to_mempool: &Sender<ConsensusEvent>,
    receiver_for_routing: Receiver<RoutingEvent>,
    sender_to_miner: &Sender<MiningEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let mut routing_controller = RoutingEventProcessor {
        blockchain: context.blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        sender_to_miner: sender_to_miner.clone(),
        io_interface: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            ROUTING_CONTROLLER_ID,
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
            routing_controller.static_peers.push(StaticPeer {
                peer_details: (*peer).clone(),
                peer_state: PeerState::Disconnected,
                peer_index: 0,
            });
        }
    }

    let (interface_sender_to_routing, interface_receiver_for_routing) =
        tokio::sync::mpsc::channel::<NetworkEvent>(1000);

    debug!("running blockchain thread");
    let routing_handle = run_thread(
        Box::new(routing_controller),
        global_sender.subscribe(),
        interface_receiver_for_routing,
        receiver_for_routing,
    )
    .await;

    (interface_sender_to_routing, routing_handle)
}

fn run_loop_thread(
    mut receiver: Receiver<IoEvent>,
    network_event_sender_to_routing: Sender<NetworkEvent>,
    network_event_sender_to_blockchain: Sender<NetworkEvent>,
    network_event_sender_to_miner: Sender<NetworkEvent>,
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
                    ROUTING_CONTROLLER_ID => {
                        debug!("routing event to blockchain controller  ",);
                        network_event_sender_to_routing
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    BLOCKCHAIN_CONTROLLER_ID => {
                        debug!("routing event to mempool controller : {:?}", command.event);
                        network_event_sender_to_blockchain
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    MINER_CONTROLLER_ID => {
                        debug!("routing event to miner controller : {:?}", command.event);
                        network_event_sender_to_miner
                            .send(command.event)
                            .await
                            .unwrap();
                    }

                    _ => {}
                }
            }

            if !work_done {
                std::thread::sleep(Duration::new(1, 0));
            } else {
                tokio::task::yield_now().await;
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

    let (event_sender_to_loop, event_receiver_in_loop) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    let (sender_to_network_controller, receiver_in_network_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    info!("running saito controllers");

    let (global_sender, global_receiver) = tokio::sync::broadcast::channel::<GlobalEvent>(1000);

    let context = Context::new(configs.clone(), global_sender.clone());

    let (sender_to_mempool, receiver_for_mempool) =
        tokio::sync::mpsc::channel::<ConsensusEvent>(1000);

    let (sender_to_routing, receiver_for_routing) =
        tokio::sync::mpsc::channel::<RoutingEvent>(1000);

    let (sender_to_miner, receiver_for_miner) = tokio::sync::mpsc::channel::<MiningEvent>(1000);

    let (network_event_sender_to_routing, routing_handle) = run_routing_controller(
        sender_to_network_controller.clone(),
        configs.clone(),
        &global_sender,
        &context,
        &sender_to_mempool,
        receiver_for_routing,
        &sender_to_miner,
    )
    .await;

    let (network_event_sender_to_blockchain, blockchain_handle) = run_blockchain_controller(
        configs.clone(),
        &global_sender,
        &context,
        receiver_for_mempool,
        &sender_to_routing,
        sender_to_miner,
        sender_to_network_controller.clone(),
    )
    .await;

    let (network_event_sender_to_miner, miner_handle) = run_miner_controller(
        &global_sender,
        &context,
        &sender_to_mempool,
        &sender_to_routing,
        receiver_for_miner,
    )
    .await;

    let loop_handle = run_loop_thread(
        event_receiver_in_loop,
        network_event_sender_to_routing,
        network_event_sender_to_blockchain,
        network_event_sender_to_miner,
    );

    let network_handle = tokio::spawn(run_network_controller(
        receiver_in_network_controller,
        event_sender_to_loop.clone(),
        configs.clone(),
        context.blockchain.clone(),
    ));

    let result = tokio::join!(
        routing_handle,
        blockchain_handle,
        miner_handle,
        loop_handle,
        network_handle
    );
    Ok(())
}

use std::panic;
use std::process;
use std::sync::Arc;

use std::time::{Duration, Instant};

use log::{debug, error, trace};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;
use tracing_subscriber;

use saito_core::common::command::NetworkEvent;
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::consensus_event_processor::{ConsensusEvent, ConsensusEventProcessor};
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::mining_event_processor::{MiningEvent, MiningEventProcessor};
use saito_core::core::routing_event_processor::{
    PeerState, RoutingEvent, RoutingEventProcessor, StaticPeer,
};

use crate::saito::config_handler::ConfigHandler;
use crate::saito::io_event::IoEvent;
use crate::saito::network_handler::run_network_controller;
use crate::saito::rust_io_handler::RustIOHandler;
use crate::saito::time_keeper::TimeKeeper;

mod saito;
mod test;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
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

async fn run_mining_event_processor(
    context: &Context,
    sender_to_mempool: &Sender<ConsensusEvent>,
    sender_to_blockchain: &Sender<RoutingEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let mining_event_processor = MiningEventProcessor {
        wallet: context.wallet.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        miner_timer: 0,
        new_miner_event_received: false,
        target: [0; 32],
        difficulty: 0,
    };
    let (interface_sender_to_miner, interface_receiver_for_miner) =
        tokio::sync::mpsc::channel::<NetworkEvent>(1000);

    debug!("running miner thread");
    let _miner_handle = run_thread(
        Box::new(mining_event_processor),
        interface_receiver_for_miner,
        receiver_for_miner,
    )
    .await;
    (interface_sender_to_miner, _miner_handle)
}

async fn run_consensus_event_processor(
    context: &Context,
    peers: Arc<RwLock<PeerCollection>>,
    receiver_for_blockchain: Receiver<ConsensusEvent>,
    sender_to_routing: &Sender<RoutingEvent>,
    sender_to_miner: Sender<MiningEvent>,
    sender_to_network_controller: Sender<IoEvent>,
    event_sender_to_loop: Sender<IoEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let result = std::env::var("GEN_TX");
    let mut create_test_tx = false;
    if result.is_ok() {
        create_test_tx = result.unwrap().eq("1");
    }
    let consensus_event_processor = ConsensusEventProcessor {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        wallet: context.wallet.clone(),
        sender_to_router: sender_to_routing.clone(),
        sender_to_miner: sender_to_miner.clone(),
        // sender_global: global_sender.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        network: Network::new(
            Box::new(RustIOHandler::new(
                sender_to_network_controller.clone(),
                CONSENSUS_EVENT_PROCESSOR_ID,
            )),
            peers.clone(),
            context.blockchain.clone(),
            context.wallet.clone(),
            context.configuration.clone(),
        ),
        block_producing_timer: 0,
        tx_producing_timer: 0,
        create_test_tx,
        storage: Storage::new(Box::new(RustIOHandler::new(
            sender_to_network_controller.clone(),
            CONSENSUS_EVENT_PROCESSOR_ID,
        ))),
    };
    let (interface_sender_to_blockchain, interface_receiver_for_mempool) =
        tokio::sync::mpsc::channel::<NetworkEvent>(1000);
    debug!("running mempool thread");
    let blockchain_handle = run_thread(
        Box::new(consensus_event_processor),
        interface_receiver_for_mempool,
        receiver_for_blockchain,
    )
    .await;

    (interface_sender_to_blockchain, blockchain_handle)
}

async fn run_routing_event_processor(
    sender_to_io_controller: Sender<IoEvent>,
    configs: Arc<RwLock<Configuration>>,
    context: &Context,
    peers: Arc<RwLock<PeerCollection>>,
    sender_to_mempool: &Sender<ConsensusEvent>,
    receiver_for_routing: Receiver<RoutingEvent>,
    sender_to_miner: &Sender<MiningEvent>,
    event_sender_to_loop: Sender<IoEvent>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let mut routing_event_processor = RoutingEventProcessor {
        blockchain: context.blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        sender_to_miner: sender_to_miner.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        static_peers: vec![],
        configs: configs.clone(),
        wallet: context.wallet.clone(),
        network: Network::new(
            Box::new(RustIOHandler::new(
                sender_to_io_controller.clone(),
                ROUTING_EVENT_PROCESSOR_ID,
            )),
            peers.clone(),
            context.blockchain.clone(),
            context.wallet.clone(),
            context.configuration.clone(),
        ),
    };
    {
        trace!("waiting for the configs write lock");
        let configs = configs.read().await;
        trace!("acquired the configs write lock");
        let peers = &configs.peers;
        for peer in peers {
            routing_event_processor.static_peers.push(StaticPeer {
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
        Box::new(routing_event_processor),
        interface_receiver_for_routing,
        receiver_for_routing,
    )
    .await;

    (interface_sender_to_routing, routing_handle)
}

// TODO : to be moved to routing event processor
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
                match command.event_processor_id {
                    ROUTING_EVENT_PROCESSOR_ID => {
                        debug!("routing event to blockchain controller  ",);
                        network_event_sender_to_routing
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    CONSENSUS_EVENT_PROCESSOR_ID => {
                        debug!("routing event to mempool controller : {:?}", command.event);
                        network_event_sender_to_blockchain
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    MINING_EVENT_PROCESSOR_ID => {
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

    let context = Context::new(configs.clone());
    let peers = Arc::new(RwLock::new(PeerCollection::new()));

    let (sender_to_mempool, receiver_for_mempool) =
        tokio::sync::mpsc::channel::<ConsensusEvent>(1000);

    let (sender_to_routing, receiver_for_routing) =
        tokio::sync::mpsc::channel::<RoutingEvent>(1000);

    let (sender_to_miner, receiver_for_miner) = tokio::sync::mpsc::channel::<MiningEvent>(1000);

    let (network_event_sender_to_routing, routing_handle) = run_routing_event_processor(
        sender_to_network_controller.clone(),
        configs.clone(),
        &context,
        peers.clone(),
        &sender_to_mempool,
        receiver_for_routing,
        &sender_to_miner,
        event_sender_to_loop.clone(),
    )
    .await;

    let (network_event_sender_to_blockchain, blockchain_handle) = run_consensus_event_processor(
        &context,
        peers.clone(),
        receiver_for_mempool,
        &sender_to_routing,
        sender_to_miner,
        sender_to_network_controller.clone(),
        event_sender_to_loop.clone(),
    )
    .await;

    let (network_event_sender_to_miner, miner_handle) = run_mining_event_processor(
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
        sender_to_network_controller.clone(),
        configs.clone(),
        context.blockchain.clone(),
        context.wallet.clone(),
        peers.clone(),
    ));

    let _result = tokio::join!(
        routing_handle,
        blockchain_handle,
        miner_handle,
        loop_handle,
        network_handle
    );
    Ok(())
}

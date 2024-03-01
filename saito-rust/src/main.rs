use std::collections::VecDeque;
use std::ops::Deref;
use std::panic;
use std::path::Path;
use std::process;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{App, Arg};
use log::info;
use log::{debug, error, trace};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing_subscriber;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::consensus::blockchain_sync_state::BlockchainSyncState;
use saito_core::core::consensus::context::Context;
use saito_core::core::consensus::peer_collection::PeerCollection;
use saito_core::core::consensus::wallet::Wallet;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::defs::{
    push_lock, Currency, PrintForLog, SaitoPrivateKey, SaitoPublicKey, StatVariable,
    LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS, PROJECT_PUBLIC_KEY, STAT_BIN_COUNT,
};
use saito_core::core::io::network::Network;
use saito_core::core::io::network_event::NetworkEvent;
use saito_core::core::io::storage::Storage;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::process::keep_time::KeepTime;
use saito_core::core::process::process_event::ProcessEvent;
use saito_core::core::routing_thread::{
    PeerState, RoutingEvent, RoutingStats, RoutingThread, StaticPeer,
};
use saito_core::core::util::configuration::Configuration;
use saito_core::core::util::crypto::generate_keys;
use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};
use saito_core::{lock_for_read, lock_for_write};
use saito_rust::saito::config_handler::{ConfigHandler, NodeConfigurations};
use saito_rust::saito::io_event::IoEvent;
use saito_rust::saito::network_controller::run_network_controller;
use saito_rust::saito::rust_io_handler::RustIOHandler;
use saito_rust::saito::stat_thread::StatThread;
use saito_rust::saito::time_keeper::TimeKeeper;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
    mut network_event_receiver: Option<Receiver<NetworkEvent>>,
    mut event_receiver: Option<Receiver<T>>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
) -> JoinHandle<()>
where
    T: Send + 'static,
{
    tokio::spawn(async move {
        info!("new thread started");
        let mut work_done;
        let mut last_timestamp = Instant::now();
        let mut stat_timer = Instant::now();
        let time_keeper = TimeKeeper {};

        event_processor.on_init().await;

        loop {
            work_done = false;
            if network_event_receiver.is_some() {
                // TODO : update to recv().await
                let result = network_event_receiver.as_mut().unwrap().try_recv();
                if result.is_ok() {
                    let event: NetworkEvent = result.unwrap();
                    if event_processor.process_network_event(event).await.is_some() {
                        work_done = true;
                    }
                }
            }

            if event_receiver.is_some() {
                // TODO : update to recv().await
                let result = event_receiver.as_mut().unwrap().try_recv();
                if result.is_ok() {
                    let event = result.unwrap();
                    if event_processor.process_event(event).await.is_some() {
                        work_done = true;
                    }
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
            #[cfg(feature = "with-stats")]
            {
                let duration = current_instant.duration_since(stat_timer);
                if duration > Duration::from_millis(stat_timer_in_ms) {
                    stat_timer = current_instant;
                    event_processor
                        .on_stat_interval(time_keeper.get_timestamp_in_ms())
                        .await;
                }
            }

            if !work_done {
                tokio::time::sleep(Duration::from_millis(thread_sleep_time_in_ms)).await;
            }
        }
    })
}

async fn run_verification_thread(
    mut event_processor: Box<VerificationThread>,
    mut event_receiver: Receiver<VerifyRequest>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("verification thread started");
        let mut work_done;
        let mut stat_timer = Instant::now();
        let time_keeper = TimeKeeper {};
        let batch_size = 10000;

        event_processor.on_init().await;
        let mut queued_requests = vec![];
        let mut requests = VecDeque::new();

        loop {
            work_done = false;

            loop {
                // TODO : update to recv().await
                let result = event_receiver.try_recv();
                if result.is_ok() {
                    let request = result.unwrap();
                    if let VerifyRequest::Block(..) = &request {
                        queued_requests.push(request);
                        break;
                    }
                    if let VerifyRequest::Transaction(tx) = request {
                        requests.push_back(tx);
                    }
                } else {
                    break;
                }
                if requests.len() == batch_size {
                    break;
                }
            }
            if !requests.is_empty() {
                event_processor
                    .processed_msgs
                    .increment_by(requests.len() as u64);
                event_processor.verify_txs(&mut requests).await;
                work_done = true;
            }
            for request in queued_requests.drain(..) {
                event_processor.process_event(request).await;
                work_done = true;
            }
            #[cfg(feature = "with-stats")]
            {
                let current_instant = Instant::now();
                let duration = current_instant.duration_since(stat_timer);
                if duration > Duration::from_millis(stat_timer_in_ms) {
                    stat_timer = current_instant;
                    event_processor
                        .on_stat_interval(time_keeper.get_timestamp_in_ms())
                        .await;
                }
            }

            if !work_done {
                tokio::time::sleep(Duration::from_millis(thread_sleep_time_in_ms)).await;
            }
        }
    })
}

async fn run_mining_event_processor(
    context: &Context,
    sender_to_mempool: &Sender<ConsensusEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
    channel_size: usize,
    sender_to_stat: Sender<String>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let mining_event_processor = MiningThread {
        wallet: context.wallet.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        miner_active: false,
        target: [0; 32],
        difficulty: 0,
        public_key: [0; 33],
        mined_golden_tickets: 0,
        stat_sender: sender_to_stat.clone(),
        configs: context.configuration.clone(),
        enabled: true,
        mining_iterations: 10_000,
    };

    let (interface_sender_to_miner, interface_receiver_for_miner) =
        tokio::sync::mpsc::channel::<NetworkEvent>(channel_size);

    debug!("running miner thread");
    let miner_handle = run_thread(
        Box::new(mining_event_processor),
        Some(interface_receiver_for_miner),
        Some(receiver_for_miner),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
    )
    .await;
    (interface_sender_to_miner, miner_handle)
}

async fn run_consensus_event_processor(
    context: &Context,
    peers: Arc<RwLock<PeerCollection>>,
    receiver_for_blockchain: Receiver<ConsensusEvent>,
    sender_to_routing: &Sender<RoutingEvent>,
    sender_to_miner: Sender<MiningEvent>,
    sender_to_network_controller: Sender<IoEvent>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
    channel_size: usize,
    sender_to_stat: Sender<String>,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let result = std::env::var("GEN_TX");
    let mut create_test_tx = false;
    if result.is_ok() {
        create_test_tx = result.unwrap().eq("1");
    }
    // let generate_genesis_block: bool;
    // {
    //     let (configs, _configs_) = lock_for_read!(context.configuration, LOCK_ORDER_CONFIGS);
    //
    //     // if we have peers defined in configs, there's already an existing network. so we don't need to generate the first block.
    //     generate_genesis_block = configs.get_peer_configs().is_empty();
    // }

    let consensus_event_processor = ConsensusThread {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        wallet: context.wallet.clone(),
        generate_genesis_block: false,
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
            context.wallet.clone(),
            context.configuration.clone(),
            Box::new(TimeKeeper {}),
        ),
        block_producing_timer: 0,
        storage: Storage::new(Box::new(RustIOHandler::new(
            sender_to_network_controller.clone(),
            CONSENSUS_EVENT_PROCESSOR_ID,
        ))),
        stats: ConsensusStats::new(sender_to_stat.clone()),
        txs_for_mempool: Vec::new(),
        stat_sender: sender_to_stat.clone(),
        configs: context.configuration.clone(),
    };
    let (interface_sender_to_blockchain, _interface_receiver_for_mempool) =
        tokio::sync::mpsc::channel::<NetworkEvent>(channel_size);
    debug!("running mempool thread");
    let blockchain_handle = run_thread(
        Box::new(consensus_event_processor),
        None,
        Some(receiver_for_blockchain),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
    )
    .await;

    (interface_sender_to_blockchain, blockchain_handle)
}

async fn run_routing_event_processor(
    sender_to_io_controller: Sender<IoEvent>,
    configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>,
    context: &Context,
    peers_lock: Arc<RwLock<PeerCollection>>,
    sender_to_mempool: &Sender<ConsensusEvent>,
    receiver_for_routing: Receiver<RoutingEvent>,
    sender_to_miner: &Sender<MiningEvent>,
    senders: Vec<Sender<VerifyRequest>>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
    channel_size: usize,
    sender_to_stat: Sender<String>,
    fetch_batch_size: usize,
) -> (Sender<NetworkEvent>, JoinHandle<()>) {
    let mut routing_event_processor = RoutingThread {
        blockchain: context.blockchain.clone(),
        mempool: context.mempool.clone(),
        sender_to_consensus: sender_to_mempool.clone(),
        sender_to_miner: sender_to_miner.clone(),
        time_keeper: Box::new(TimeKeeper {}),
        static_peers: vec![],
        configs: configs_lock.clone(),
        wallet: context.wallet.clone(),
        network: Network::new(
            Box::new(RustIOHandler::new(
                sender_to_io_controller.clone(),
                ROUTING_EVENT_PROCESSOR_ID,
            )),
            peers_lock.clone(),
            context.wallet.clone(),
            configs_lock.clone(),
            Box::new(TimeKeeper {}),
        ),
        reconnection_timer: 0,
        stats: RoutingStats::new(sender_to_stat.clone()),
        senders_to_verification: senders,
        last_verification_thread_index: 0,
        stat_sender: sender_to_stat.clone(),
        blockchain_sync_state: BlockchainSyncState::new(fetch_batch_size),
        initial_connection: false,
        reconnection_wait_time: 0,
    };

    {
        let (configs, _configs_) = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);
        routing_event_processor.reconnection_wait_time =
            configs.get_server_configs().unwrap().reconnection_wait_time;
        let peers = configs.get_peer_configs();
        for peer in peers {
            routing_event_processor.static_peers.push(StaticPeer {
                peer_details: (*peer).clone(),
                peer_state: PeerState::Disconnected,
                peer_index: 0,
            });
        }
    }

    let (interface_sender_to_routing, interface_receiver_for_routing) =
        tokio::sync::mpsc::channel::<NetworkEvent>(channel_size);

    debug!("running blockchain thread");
    let routing_handle = run_thread(
        Box::new(routing_event_processor),
        Some(interface_receiver_for_routing),
        Some(receiver_for_routing),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
    )
    .await;

    (interface_sender_to_routing, routing_handle)
}

async fn run_verification_threads(
    sender_to_consensus: Sender<ConsensusEvent>,
    blockchain: Arc<RwLock<Blockchain>>,
    peers: Arc<RwLock<PeerCollection>>,
    wallet: Arc<RwLock<Wallet>>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
    verification_thread_count: u16,
    sender_to_stat: Sender<String>,
) -> (Vec<Sender<VerifyRequest>>, Vec<JoinHandle<()>>) {
    let mut senders = vec![];
    let mut thread_handles = vec![];

    for i in 0..verification_thread_count {
        let (sender, receiver) = tokio::sync::mpsc::channel(1_000_000);
        senders.push(sender);
        let verification_thread = VerificationThread {
            sender_to_consensus: sender_to_consensus.clone(),
            blockchain: blockchain.clone(),
            peers: peers.clone(),
            wallet: wallet.clone(),
            processed_txs: StatVariable::new(
                format!("verification_{:?}::processed_txs", i),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            processed_blocks: StatVariable::new(
                format!("verification_{:?}::processed_blocks", i),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            processed_msgs: StatVariable::new(
                format!("verification_{:?}::processed_msgs", i),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            invalid_txs: StatVariable::new(
                format!("verification_{:?}::invalid_txs", i),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            stat_sender: sender_to_stat.clone(),
        };

        let thread_handle = run_verification_thread(
            Box::new(verification_thread),
            receiver,
            stat_timer_in_ms,
            thread_sleep_time_in_ms,
        )
        .await;
        thread_handles.push(thread_handle);
    }

    (senders, thread_handles)
}

// TODO : to be moved to routing event processor
fn run_loop_thread(
    mut receiver: Receiver<IoEvent>,
    network_event_sender_to_routing_ep: Sender<NetworkEvent>,
    stat_timer_in_ms: u64,
    thread_sleep_time_in_ms: u64,
    sender_to_stat: Sender<String>,
) -> JoinHandle<()> {
    let loop_handle = tokio::spawn(async move {
        let mut work_done: bool;
        let mut incoming_msgs = StatVariable::new(
            "network::incoming_msgs".to_string(),
            STAT_BIN_COUNT,
            sender_to_stat.clone(),
        );
        let mut last_stat_on: Instant = Instant::now();
        loop {
            work_done = false;

            let result = receiver.recv().await;
            if result.is_some() {
                let command = result.unwrap();
                incoming_msgs.increment();
                work_done = true;
                // TODO : remove hard coded values
                match command.event_processor_id {
                    ROUTING_EVENT_PROCESSOR_ID => {
                        trace!("routing event to routing event processor  ",);
                        network_event_sender_to_routing_ep
                            .send(command.event)
                            .await
                            .unwrap();
                    }
                    CONSENSUS_EVENT_PROCESSOR_ID => {
                        trace!(
                            "routing event to consensus event processor : {:?}",
                            command.event
                        );
                        unreachable!()
                        // network_event_sender_to_consensus_ep
                        //     .send(command.event)
                        //     .await
                        //     .unwrap();
                    }
                    MINING_EVENT_PROCESSOR_ID => {
                        trace!(
                            "routing event to mining event processor : {:?}",
                            command.event
                        );
                        unreachable!()
                        // network_event_sender_to_mining_ep
                        //     .send(command.event)
                        //     .await
                        //     .unwrap();
                    }

                    _ => {}
                }
            }
            #[cfg(feature = "with-stats")]
            {
                if Instant::now().duration_since(last_stat_on)
                    > Duration::from_millis(stat_timer_in_ms)
                {
                    last_stat_on = Instant::now();
                    incoming_msgs
                        .calculate_stats(TimeKeeper {}.get_timestamp_in_ms())
                        .await;
                }
            }
            if !work_done {
                tokio::time::sleep(Duration::from_millis(thread_sleep_time_in_ms)).await;
            }
        }
    });

    loop_handle
}

fn setup_log() {
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
}

fn setup_hook() {
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
}

async fn run_node(configs_lock: Arc<RwLock<dyn Configuration + Send + Sync>>) {
    info!("Running saito with config {:?}", configs_lock.read().await);

    let channel_size;
    let thread_sleep_time_in_ms;
    let stat_timer_in_ms;
    let verification_thread_count;
    let fetch_batch_size;

    {
        let (configs, _configs_) = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);

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

    info!("running saito controllers");

    let keys = generate_keys();
    let wallet_lock = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
    {
        let mut wallet = wallet_lock.write().await;
        Wallet::load(
            &mut wallet,
            Box::new(RustIOHandler::new(
                sender_to_network_controller.clone(),
                ROUTING_EVENT_PROCESSOR_ID,
            )),
        )
        .await;
    }
    let context = Context::new(configs_lock.clone(), wallet_lock);

    let peers_lock = Arc::new(RwLock::new(PeerCollection::new()));

    let (sender_to_consensus, receiver_for_consensus) =
        tokio::sync::mpsc::channel::<ConsensusEvent>(channel_size);

    let (sender_to_routing, receiver_for_routing) =
        tokio::sync::mpsc::channel::<RoutingEvent>(channel_size);

    let (sender_to_miner, receiver_for_miner) =
        tokio::sync::mpsc::channel::<MiningEvent>(channel_size);
    let (sender_to_stat, receiver_for_stat) = tokio::sync::mpsc::channel::<String>(channel_size);

    let (senders, verification_handles) = run_verification_threads(
        sender_to_consensus.clone(),
        context.blockchain.clone(),
        peers_lock.clone(),
        context.wallet.clone(),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
        verification_thread_count,
        sender_to_stat.clone(),
    )
    .await;

    let (network_event_sender_to_routing, routing_handle) = run_routing_event_processor(
        sender_to_network_controller.clone(),
        configs_lock.clone(),
        &context,
        peers_lock.clone(),
        &sender_to_consensus,
        receiver_for_routing,
        &sender_to_miner,
        senders,
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
        channel_size,
        sender_to_stat.clone(),
        fetch_batch_size,
    )
    .await;

    let (_network_event_sender_to_consensus, blockchain_handle) = run_consensus_event_processor(
        &context,
        peers_lock.clone(),
        receiver_for_consensus,
        &sender_to_routing,
        sender_to_miner,
        sender_to_network_controller.clone(),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
        channel_size,
        sender_to_stat.clone(),
    )
    .await;

    let (_network_event_sender_to_mining, miner_handle) = run_mining_event_processor(
        &context,
        &sender_to_consensus,
        receiver_for_miner,
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
        channel_size,
        sender_to_stat.clone(),
    )
    .await;
    let stat_handle = run_thread(
        Box::new(StatThread::new().await),
        None,
        Some(receiver_for_stat),
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
    )
    .await;
    let loop_handle: JoinHandle<()> = run_loop_thread(
        event_receiver_in_loop,
        network_event_sender_to_routing,
        stat_timer_in_ms,
        thread_sleep_time_in_ms,
        sender_to_stat.clone(),
    );

    let network_handle = tokio::spawn(run_network_controller(
        receiver_in_network_controller,
        event_sender_to_loop.clone(),
        configs_lock.clone(),
        context.blockchain.clone(),
        sender_to_stat.clone(),
        peers_lock.clone(),
    ));

    let _result = tokio::join!(
        routing_handle,
        blockchain_handle,
        miner_handle,
        loop_handle,
        network_handle,
        stat_handle,
        futures::future::join_all(verification_handles)
    );
}

pub async fn run_utxo_to_issuance_converter(threshold: Currency) {
    let (sender_to_network_controller, _receiver_in_network_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(100);

    info!("running saito controllers");
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

    let configs_lock: Arc<RwLock<NodeConfigurations>> =
        Arc::new(RwLock::new(NodeConfigurations::default()));

    let configs_clone: Arc<RwLock<dyn Configuration + Send + Sync>> = configs_lock.clone();

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
    let list = storage.load_block_name_list().await.unwrap();
    storage
        .load_blocks_from_disk(list, context.mempool.clone())
        .await;

    let _peers_lock = Arc::new(RwLock::new(PeerCollection::new()));

    let (_sender_to_miner, _receiver_for_miner) = tokio::sync::mpsc::channel::<MiningEvent>(100);

    let (configs, _configs_) = lock_for_read!(configs_lock, LOCK_ORDER_CONFIGS);

    let (mut blockchain, _blockchain_) = lock_for_write!(context.blockchain, LOCK_ORDER_BLOCKCHAIN);
    blockchain
        .add_blocks_from_mempool(
            context.mempool.clone(),
            None,
            &mut storage,
            None,
            configs.deref(),
        )
        .await;

    let data = blockchain.get_utxoset_data();

    info!("{:?} entries in utxo to write to file", data.len());
    let issuance_path: String = "./data/issuance.file".to_string();
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
        return;
    }
    let mut file = file.unwrap();

    let slip_type = "Normal";
    let mut aggregated_value = 0;
    let mut total_written_lines = 0;
    for (key, value) in &data {
        if value < &threshold {
            // PROJECT_PUBLIC_KEY.to_string()
            aggregated_value += value;
        } else {
            total_written_lines += 1;
            let key_base58 = key.to_base58();

            file.write_all(format!("{}\t{}\t{}\n", value, key_base58, slip_type).as_bytes())
                .await
                .expect("failed writing to issuance file");
        };
    }

    // add remaining value
    if aggregated_value > 0 {
        total_written_lines += 1;
        file.write_all(
            format!(
                "{}\t{}\t{}\n",
                aggregated_value,
                PROJECT_PUBLIC_KEY.to_string(),
                slip_type
            )
            .as_bytes(),
        )
        .await
        .expect("failed writing to issuance file");
    }

    file.flush()
        .await
        .expect("failed flushing issuance file data");

    info!("total written lines : {:?}", total_written_lines);
}
#[tokio::main(flavor = "multi_thread")]
// #[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("Saito")
        .arg(
            Arg::with_name("config")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mode")
                .long("mode")
                .value_name("MODE")
                .default_value("node")
                .possible_values(["node", "utxo-issuance"])
                .help("Sets the mode for execution")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("utxo_threshold")
                .long("threshold")
                .value_name("UTXO_THRESHOLD")
                .help("Threshold for selecting utxo for issuance file")
                .takes_value(true),
        )
        .get_matches();

    let program_mode = matches.value_of("mode").unwrap_or("node");

    setup_log();

    setup_hook();

    if program_mode == "node" {
        let config_file = matches.value_of("config").unwrap_or("configs/config.json");
        info!("Using config file: {}", config_file.to_string());
        let configs = ConfigHandler::load_configs(config_file.to_string());
        if configs.is_err() {
            error!("failed loading configs. {:?}", configs.err().unwrap());
            return Ok(());
        }

        let configs: Arc<RwLock<dyn Configuration + Send + Sync>> =
            Arc::new(RwLock::new(configs.unwrap()));

        run_node(configs).await;
    } else if program_mode == "utxo-issuance" {
        let threshold_str = matches.value_of("utxo_threshold");
        let mut threshold: Currency = 25_000;
        if threshold_str.is_some() {
            let result = String::from(threshold_str.unwrap()).parse();
            if result.is_err() {
                error!("cannot parse threshold : {:?}", threshold_str);
            } else {
                threshold = result.unwrap();
            }
        }
        info!(
            "running the program in utxo to issuance converter mode with threshold : {:?}",
            threshold
        );

        run_utxo_to_issuance_converter(threshold).await;
    }

    Ok(())
}

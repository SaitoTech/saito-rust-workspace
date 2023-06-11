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
use saito_core::common::keep_time::KeepTime;
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::blockchain_sync_state::BlockchainSyncState;
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
use saito_rust::saito::stat_thread::StatThread;
use saito_rust::saito::time_keeper::TimeKeeper;
use crate::saito::config_handler::SpammerConfigs;

const ROUTING_EVENT_PROCESSOR_ID: u8 = 1;
const CONSENSUS_EVENT_PROCESSOR_ID: u8 = 2;
const MINING_EVENT_PROCESSOR_ID: u8 = 3;

//mod saito;
//mod thread_util;

use crate::run_thread;
use crate::run_loop_thread;


pub async fn run_verification_thread(
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

pub async fn run_mining_event_processor(
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

pub async fn run_consensus_event_processor(
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
    let generate_genesis_block: bool;
    {
        let (configs, _configs_) = lock_for_read!(context.configuration, LOCK_ORDER_CONFIGS);

        // if we have peers defined in configs, there's already an existing network. so we don't need to generate the first block.
        generate_genesis_block = configs.get_peer_configs().is_empty();
    }

    let consensus_event_processor = ConsensusThread {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        wallet: context.wallet.clone(),
        generate_genesis_block,
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
        ),
        block_producing_timer: 0,
        tx_producing_timer: 0,
        create_test_tx,
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

pub async fn run_routing_event_processor(
    sender_to_io_controller: Sender<IoEvent>,
    configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
    context: &Context,
    peers: Arc<RwLock<PeerCollection>>,
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
        sender_to_consensus: sender_to_mempool.clone(),
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
            context.wallet.clone(),
            configs.clone(),
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
        let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);
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

pub async fn run_verification_threads(
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

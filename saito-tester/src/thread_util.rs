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
use saito_core::common::defs::{
    push_lock, SaitoPrivateKey, SaitoPublicKey, StatVariable, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_WALLET, STAT_BIN_COUNT,
};
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
use saito_core::{lock_for_read, lock_for_write};
use saito_rust::saito::io_event::IoEvent;
use saito_rust::saito::network_controller::run_network_controller;

use saito_rust::saito::rust_io_handler::RustIOHandler;
use saito_rust::saito::stat_thread::StatThread;
use saito_rust::saito::time_keeper::TimeKeeper;

use crate::saito::config_handler::{ConfigHandler, SpammerConfigs};
// use crate::saito::rust_io_handler::RustIOHandler;
//use crate::saito::spammer::run_spammer;
//use crate::saito::config_handler::SpammerConfigs;

pub async fn run_thread<T>(
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
        let mut work_done = false;
        let mut last_timestamp = Instant::now();
        let mut stat_timer = Instant::now();
        let time_keeper = TimeKeeper {};

        event_processor.on_init().await;

        loop {
            if network_event_receiver.is_some() {
                // TODO : update to recv().await
                let result = network_event_receiver.as_mut().unwrap().try_recv();
                if result.is_ok() {
                    let event = result.unwrap();
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

            if work_done {
                work_done = false;
                // tokio::task::yield_now().await;
            } else {
                // tokio::task::yield_now().await;
                tokio::time::sleep(Duration::from_millis(thread_sleep_time_in_ms)).await;
            }
        }
    })
}

// TODO : to be moved to routing event processor
pub fn run_loop_thread(
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
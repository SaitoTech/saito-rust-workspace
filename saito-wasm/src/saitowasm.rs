use std::collections::HashMap;
use std::future::Future;
use std::io::Error;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Poll, Waker};
use std::time::Duration;

use js_sys::{Array, BigInt, Uint8Array};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};
use wasm_bindgen::prelude::*;

use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, SaitoPublicKey, SaitoSignature, LOCK_ORDER_PEERS,
    LOCK_ORDER_WALLET,
};
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::blockchain_sync_state::BlockchainSyncState;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::routing_thread::{RoutingEvent, RoutingStats, RoutingThread};
use saito_core::lock_for_write;
use wasm_bindgen_console_logger::DEFAULT_LOGGER;

use crate::wasm_configuration::WasmConfiguration;
use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_slip::WasmSlip;
use crate::wasm_task_runner::WasmTaskRunner;
use crate::wasm_time_keeper::WasmTimeKeeper;
use crate::wasm_transaction::WasmTransaction;
use log::info;

pub(crate) struct NetworkResultFuture {
    pub result: Option<Result<Vec<u8>, Error>>,
    pub key: u64,
}

// TODO : check if this gets called from somewhere or need a runtime
impl Future for NetworkResultFuture {
    type Output = Result<Vec<u8>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut saito = SAITO.blocking_lock();
        let result = saito.results.remove(&self.key);
        if result.is_some() {
            let result = result.unwrap();
            return Poll::Ready(result);
        }
        let waker = cx.waker().clone();
        saito.wakers.insert(self.key, waker);
        return Poll::Pending;
    }
}

#[wasm_bindgen]
pub struct SaitoWasm {
    routing_thread: RoutingThread,
    consensus_thread: ConsensusThread,
    mining_thread: MiningThread,
    receiver_for_router: Receiver<RoutingEvent>,
    receiver_for_consensus: Receiver<ConsensusEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
    context: Context,
    wakers: HashMap<u64, Waker>,
    results: HashMap<u64, Result<Vec<u8>, Error>>,
}

lazy_static! {
    static ref SAITO: Mutex<SaitoWasm> = Mutex::new(new());
    static ref CONFIGS: Arc<RwLock<Box<dyn Configuration + Send + Sync>>> =
        Arc::new(RwLock::new(Box::new(WasmConfiguration::new())));
}

// #[wasm_bindgen]
// impl SaitoWasm {}

pub fn new() -> SaitoWasm {
    let wallet = Wallet::new();
    let public_key = wallet.public_key.clone();
    let private_key = wallet.private_key.clone();
    let wallet = Arc::new(RwLock::new(wallet));
    let configuration: Arc<RwLock<Box<dyn Configuration + Send + Sync>>> = CONFIGS.clone();

    let peers = Arc::new(RwLock::new(PeerCollection::new()));
    let context = Context {
        blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
        mempool: Arc::new(RwLock::new(Mempool::new(public_key, private_key))),
        wallet: wallet.clone(),
        configuration: configuration.clone(),
    };

    let (sender_to_mempool, receiver_in_mempool) = tokio::sync::mpsc::channel(100);
    let (sender_to_blockchain, receiver_in_blockchain) = tokio::sync::mpsc::channel(100);
    let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(100);
    let (sender_to_stat, receiver_in_stats) = tokio::sync::mpsc::channel(100);

    SaitoWasm {
        routing_thread: RoutingThread {
            blockchain: context.blockchain.clone(),
            sender_to_consensus: sender_to_mempool.clone(),
            sender_to_miner: sender_to_miner.clone(),
            static_peers: vec![],
            configs: context.configuration.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            wallet,
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet.clone(),
            ),
            reconnection_timer: 0,
            stats: RoutingStats::new(sender_to_stat.clone()),
            public_key,
            senders_to_verification: vec![],
            last_verification_thread_index: 0,
            stat_sender: sender_to_stat.clone(),
            blockchain_sync_state: BlockchainSyncState::new(10),
        },
        consensus_thread: ConsensusThread {
            mempool: context.mempool.clone(),
            blockchain: context.blockchain.clone(),
            wallet: context.wallet.clone(),
            generate_genesis_block: false,
            sender_to_router: sender_to_blockchain.clone(),
            sender_to_miner: sender_to_miner.clone(),
            // sender_global: (),
            block_producing_timer: 0,
            tx_producing_timer: 0,
            create_test_tx: false,
            time_keeper: Box::new(WasmTimeKeeper {}),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet.clone(),
            ),
            storage: Storage::new(Box::new(WasmIoHandler {})),
            stats: ConsensusStats::new(sender_to_stat.clone()),
            txs_for_mempool: vec![],
            stat_sender: sender_to_stat.clone(),
        },
        mining_thread: MiningThread {
            wallet: context.wallet.clone(),

            sender_to_mempool: sender_to_mempool.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            miner_active: false,
            target: [0; 32],
            difficulty: 0,
            public_key: [0; 33],
            mined_golden_tickets: 0,
            stat_sender: sender_to_stat.clone(),
        },
        receiver_for_router: receiver_in_blockchain,
        receiver_for_consensus: receiver_in_mempool,
        receiver_for_miner: receiver_in_miner,
        context,
        wakers: Default::default(),
        results: Default::default(),
    }
}

#[wasm_bindgen]
pub async fn set_configs(config_string: js_sys::JsString) {
    let mut configs = CONFIGS.write().await;
    let config = WasmConfiguration::new();
    let config: Box<dyn Configuration + Send + Sync> = Box::new(config);
    let _ = std::mem::replace(&mut configs.deref(), &config);
}

#[wasm_bindgen]
pub async fn initialize() -> Result<JsValue, JsValue> {
    log::set_logger(&DEFAULT_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    info!("initializing sakviti-wasm");

    let mut saito = SAITO.lock().await;
    saito.mining_thread.on_init().await;
    saito.consensus_thread.on_init().await;
    saito.routing_thread.on_init().await;

    return Ok(JsValue::from("initialized"));
}

#[wasm_bindgen]
pub async fn create_transaction() -> Result<WasmTransaction, JsValue> {
    let saito = SAITO.lock().await;
    let (mut wallet, _wallet_) = lock_for_write!(saito.context.wallet, LOCK_ORDER_WALLET);
    let transaction = wallet.create_transaction_with_default_fees().await;
    let wasm_transaction = WasmTransaction::from_transaction(transaction);
    return Ok(wasm_transaction);
}

#[wasm_bindgen]
pub async fn send_transaction(transaction: WasmTransaction) -> Result<JsValue, JsValue> {
    // todo : convert transaction

    let saito = SAITO.lock().await;
    // saito.blockchain_controller.
    Ok(JsValue::from("test"))
}

#[wasm_bindgen]
pub fn get_latest_block_hash() -> Result<JsValue, JsValue> {
    Ok(JsValue::from("latestblockhash"))
}

#[wasm_bindgen]
pub fn get_public_key() -> Result<JsValue, JsValue> {
    Ok(JsValue::from("public_key"))
}

#[wasm_bindgen]
pub async fn process_timer_event(duration: u64) {
    // println!("processing timer event : {:?}", duration);

    let mut saito = SAITO.lock().await;

    let duration = Duration::new(0, 1_000_000 * duration as u32);

    // blockchain controller
    // TODO : update to recv().await
    let result = saito.receiver_for_router.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let result = saito.routing_thread.process_event(event).await;
    }

    saito
        .routing_thread
        .process_timer_event(duration.clone())
        .await;
    // mempool controller
    // TODO : update to recv().await
    let result = saito.receiver_for_consensus.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let result = saito.consensus_thread.process_event(event).await;
    }
    saito
        .consensus_thread
        .process_timer_event(duration.clone())
        .await;

    // miner controller
    let result = saito.receiver_for_miner.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let result = saito.mining_thread.process_event(event).await;
    }
    saito.mining_thread.process_timer_event(duration.clone());
}

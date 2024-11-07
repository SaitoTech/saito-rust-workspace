use std::io::{Error, ErrorKind};

use std::sync::Arc;
use std::time::Duration;

use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use pyo3::pyfunction;
use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::consensus::blockchain_sync_state::BlockchainSyncState;
use saito_core::core::consensus::context::Context;
use saito_core::core::consensus::mempool::Mempool;
use saito_core::core::consensus::peers::peer_collection::PeerCollection;
use saito_core::core::consensus::wallet::Wallet;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::defs::{
    Currency, PeerIndex, PrintForLog, SaitoPrivateKey, SaitoPublicKey, StatVariable, Timestamp,
    PROJECT_PUBLIC_KEY, STAT_BIN_COUNT,
};
use saito_core::core::io::network::{Network, PeerDisconnectType};
use saito_core::core::io::network_event::NetworkEvent;
use saito_core::core::io::storage::Storage;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::process::keep_time::Timer;
use saito_core::core::process::process_event::ProcessEvent;
use saito_core::core::routing_thread::{RoutingEvent, RoutingStats, RoutingThread};
use saito_core::core::stat_thread::StatThread;
use saito_core::core::util::configuration::Configuration;
use saito_core::core::util::crypto::generate_keypair_from_private_key;
use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};
use secp256k1::SECP256K1;
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};

use crate::wasm_balance_snapshot::WasmBalanceSnapshot;
use crate::wasm_configuration::WasmConfiguration;
use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_peer::WasmPeer;
use crate::wasm_time_keeper::WasmTimeKeeper;
use crate::wasm_transaction::WasmTransaction;

pub struct SaitoWasm {
    pub(crate) routing_thread: RoutingThread,
    consensus_thread: ConsensusThread,
    mining_thread: MiningThread,
    verification_thread: VerificationThread,
    stat_thread: StatThread,
    receiver_for_router: Receiver<RoutingEvent>,
    receiver_for_consensus: Receiver<ConsensusEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
    receiver_for_verification: Receiver<VerifyRequest>,
    receiver_for_stats: Receiver<String>,
    pub(crate) context: Context,
    // wallet: WasmWallet,
    // blockchain: WasmBlockchain,
}

lazy_static! {
    pub static ref SAITO: Mutex<Option<SaitoWasm>> = Mutex::new(Some(new(1, true)));
    static ref CONFIGS: Arc<RwLock<dyn Configuration + Send + Sync>> =
        Arc::new(RwLock::new(WasmConfiguration::new()));
    static ref PRIVATE_KEY: Mutex<String> = Mutex::new("".to_string());
}

pub fn new(haste_multiplier: u64, enable_stats: bool) -> SaitoWasm {
    info!("creating new saito wasm instance");

    let wallet = Arc::new(RwLock::new(Wallet::new([0; 32], [0; 33])));

    let configuration: Arc<RwLock<dyn Configuration + Send + Sync>> = CONFIGS.clone();

    let channel_size = 1_000_000;

    let peers = Arc::new(RwLock::new(PeerCollection::default()));
    let context = Context {
        blockchain_lock: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
        mempool_lock: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
        wallet_lock: wallet.clone(),
        config_lock: configuration.clone(),
    };

    let (sender_to_consensus, receiver_in_mempool) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_blockchain, receiver_in_blockchain) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_stat, receiver_in_stats) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_verification, receiver_in_verification) =
        tokio::sync::mpsc::channel(channel_size);

    let timer = Timer {
        time_reader: Arc::new(WasmTimeKeeper {}),
        hasten_multiplier: haste_multiplier,
        start_time: 0 as Timestamp,
    };

    SaitoWasm {
        routing_thread: RoutingThread {
            blockchain_lock: context.blockchain_lock.clone(),
            mempool_lock: context.mempool_lock.clone(),
            sender_to_consensus: sender_to_consensus.clone(),
            sender_to_miner: sender_to_miner.clone(),
            config_lock: context.config_lock.clone(),
            timer: timer.clone(),
            wallet_lock: wallet.clone(),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet_lock.clone(),
                context.config_lock.clone(),
                timer.clone(),
            ),
            reconnection_timer: 0,
            peer_removal_timer: 0,
            peer_file_write_timer: 0,
            last_emitted_block_fetch_count: 0,
            stats: RoutingStats::new(sender_to_stat.clone()),
            senders_to_verification: vec![sender_to_verification.clone()],
            last_verification_thread_index: 0,
            stat_sender: sender_to_stat.clone(),
            blockchain_sync_state: BlockchainSyncState::new(10),
        },
        consensus_thread: ConsensusThread {
            mempool_lock: context.mempool_lock.clone(),
            blockchain_lock: context.blockchain_lock.clone(),
            wallet_lock: context.wallet_lock.clone(),
            generate_genesis_block: false,
            sender_to_router: sender_to_blockchain.clone(),
            sender_to_miner: sender_to_miner.clone(),
            // sender_global: (),
            block_producing_timer: 0,
            timer: timer.clone(),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet_lock.clone(),
                configuration.clone(),
                timer.clone(),
            ),
            storage: Storage::new(Box::new(WasmIoHandler {})),
            stats: ConsensusStats::new(sender_to_stat.clone()),
            txs_for_mempool: vec![],
            stat_sender: sender_to_stat.clone(),
            config_lock: configuration.clone(),
            produce_blocks_by_timer: true,
        },
        mining_thread: MiningThread {
            wallet_lock: context.wallet_lock.clone(),
            sender_to_mempool: sender_to_consensus.clone(),
            timer: timer.clone(),
            miner_active: false,
            target: [0; 32],
            target_id: 0,
            difficulty: 0,
            public_key: [0; 33],
            mined_golden_tickets: 0,
            stat_sender: sender_to_stat.clone(),
            config_lock: configuration.clone(),
            enabled: true,
            mining_iterations: 1_000,
            mining_start: 0,
        },
        verification_thread: VerificationThread {
            sender_to_consensus: sender_to_consensus.clone(),
            blockchain_lock: context.blockchain_lock.clone(),
            peer_lock: peers.clone(),
            wallet_lock: wallet.clone(),
            processed_txs: StatVariable::new(
                "verification::processed_txs".to_string(),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            processed_blocks: StatVariable::new(
                "verification::processed_blocks".to_string(),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            processed_msgs: StatVariable::new(
                "verification::processed_msgs".to_string(),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            invalid_txs: StatVariable::new(
                "verification::invalid_txs".to_string(),
                STAT_BIN_COUNT,
                sender_to_stat.clone(),
            ),
            stat_sender: sender_to_stat.clone(),
        },
        stat_thread: StatThread {
            stat_queue: Default::default(),
            io_interface: Box::new(WasmIoHandler {}),
            enabled: enable_stats,
        },
        receiver_for_router: receiver_in_blockchain,
        receiver_for_consensus: receiver_in_mempool,
        receiver_for_miner: receiver_in_miner,
        receiver_for_verification: receiver_in_verification,
        // wallet: WasmWallet::new_from(
        //     context.wallet_lock.clone(),
        //     Network::new(
        //         Box::new(WasmIoHandler {}),
        //         peers.clone(),
        //         context.wallet_lock.clone(),
        //         configuration.clone(),
        //         timer.clone(),
        //     ),
        // ),
        // blockchain: WasmBlockchain {
        //     blockchain_lock: context.blockchain_lock.clone(),
        // },
        context,
        receiver_for_stats: receiver_in_stats,
    }
}
//
// struct WasmLogger {}
//
// impl Log for WasmLogger {
//     fn enabled(&self, metadata: &Metadata) -> bool {
//         metadata.level() <= log::max_level()
//     }
//
//     fn log(&self, record: &Record) {
//         if !self.enabled(record.metadata()) {
//             return;
//         }
//         log(record)
//     }
//
//     fn flush(&self) {}
// }
pub(crate) struct Style<'s> {
    pub trace: &'s str,
    pub debug: &'s str,
    pub info: &'s str,
    pub warn: &'s str,
    pub error: &'s str,
    pub file_line: &'s str,
    pub text: &'s str,
}

impl Style<'static> {
    /// Returns default style values.
    pub const fn default() -> Self {
        macro_rules! bg_color {
            ($color:expr) => {
                concat!("color: white; padding: 0 3px; background: ", $color, ";")
            };
        }

        Style {
            trace: bg_color!("gray"),
            debug: bg_color!("blue"),
            info: bg_color!("green"),
            warn: bg_color!("orange"),
            error: bg_color!("darkred"),
            file_line: "font-weight: bold; color: inherit",
            text: "background: inherit; color: inherit",
        }
    }
}
const STYLE: Style<'static> = Style::default();

// pub fn log(record: &Record) {
//     let console_log = match record.level() {
//         Level::Error => console::error_4,
//         Level::Warn => console::warn_4,
//         Level::Info => console::info_4,
//         Level::Debug => console::log_4,
//         Level::Trace => console::debug_4,
//     };
//
//     let message = {
//         let message = format!(
//             "%c%c%c{text}",
//             // level = record.level(),
//             // file = record.file().unwrap_or_else(|| record.target()),
//             // line = record
//             //     .line()
//             //     .map_or_else(|| "[Unknown]".to_string(), |line| line.to_string()),
//             text = record.args(),
//         );
//         JsValue::from(&message)
//     };
//
//     let level_style = {
//         let style_str = match record.level() {
//             Level::Trace => STYLE.trace,
//             Level::Debug => STYLE.debug,
//             Level::Info => STYLE.info,
//             Level::Warn => STYLE.warn,
//             Level::Error => STYLE.error,
//         };
//
//         JsValue::from(style_str)
//     };
//
//     let file_line_style = JsValue::from_str(STYLE.file_line);
//     let text_style = JsValue::from_str(STYLE.text);
//     console_log(&message, &level_style, &file_line_style, &text_style);
// }

#[pyfunction]
pub async fn initialize(
    json: String,
    private_key: String,
    log_level_num: u8,
    hasten_multiplier: u64,
) {
    // let log_level = match log_level_num {
    //     0 => log::Level::Error,
    //     1 => log::Level::Warn,
    //     2 => log::Level::Info,
    //     3 => log::Level::Debug,
    //     4 => log::Level::Trace,
    //     _ => log::Level::Info,
    // };
    //
    // log::set_logger(&WasmLogger {}).unwrap();
    // log::set_max_level(log_level.to_level_filter());

    // console_log::init_with_level(log_level).unwrap();

    trace!("trace test");
    debug!("debug test");
    info!("initializing saito-wasm");

    let mut enable_stats = true;
    {
        info!("setting configs...");
        let mut configs = CONFIGS.write().await;
        info!("config lock acquired");

        let str: String = json.into();
        let config = WasmConfiguration::new_from_json(str.as_str());

        if config.is_err() {
            error!("failed parsing configs. {:?}", config.err().unwrap());
        } else {
            let config = config.unwrap();
            if config.is_browser() {
                enable_stats = false;
            }
            info!("config : {:?}", config);
            configs.replace(&config);
        }
    }

    let mut saito = SAITO.lock().await;

    saito.replace(new(hasten_multiplier, enable_stats));

    let private_key: SaitoPrivateKey = string_to_hex(private_key).unwrap();
    {
        let mut wallet = saito.as_ref().unwrap().context.wallet_lock.write().await;
        if private_key != [0; 32] {
            let keys = generate_keypair_from_private_key(private_key.as_slice());
            wallet.private_key = keys.1;
            wallet.public_key = keys.0;
        }
        info!("current core version : {:?}", wallet.core_version);
    }

    saito.as_mut().unwrap().stat_thread.on_init().await;
    saito.as_mut().unwrap().mining_thread.on_init().await;
    saito.as_mut().unwrap().verification_thread.on_init().await;
    saito.as_mut().unwrap().routing_thread.on_init().await;
    saito.as_mut().unwrap().consensus_thread.on_init().await;
}

// pub async fn create_transaction(
//     public_key: String,
//     amount: u64,
//     fee: u64,
//     force_merge: bool,
// ) -> Result<WasmTransaction, JsValue> {
//     trace!("create_transaction : {:?}", public_key.to_string());
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.as_ref().unwrap().context.wallet_lock.write().await;
//     let key = string_to_key(public_key).or(Err(JsValue::from(
//         "Failed parsing public key string to key",
//     )))?;
//
//     let transaction = Transaction::create(
//         &mut wallet,
//         key,
//         amount,
//         fee,
//         force_merge,
//         Some(&saito.as_ref().unwrap().consensus_thread.network),
//     );
//     if transaction.is_err() {
//         error!(
//             "failed creating transaction. {:?}",
//             transaction.err().unwrap()
//         );
//         return Err(JsValue::from("Failed creating transaction"));
//     }
//     let transaction = transaction.unwrap();
//     let wasm_transaction = WasmTransaction::from_transaction(transaction);
//     Ok(wasm_transaction)
// }
//
// pub async fn create_transaction_with_multiple_payments(
//     public_keys: js_sys::Array,
//     amounts: js_sys::BigUint64Array,
//     fee: u64,
//     _force_merge: bool,
// ) -> Result<WasmTransaction, JsValue> {
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.as_ref().unwrap().context.wallet_lock.write().await;
//
//     let keys: Vec<SaitoPublicKey> = string_array_to_base58_keys(public_keys);
//     let amounts: Vec<Currency> = amounts.to_vec();
//
//     if keys.len() != amounts.len() {
//         return Err(JsValue::from("keys and payments have different counts"));
//     }
//
//     let transaction = Transaction::create_with_multiple_payments(
//         &mut wallet,
//         keys,
//         amounts,
//         fee,
//         Some(&saito.as_ref().unwrap().consensus_thread.network),
//     );
//     if transaction.is_err() {
//         error!(
//             "failed creating transaction. {:?}",
//             transaction.err().unwrap()
//         );
//         return Err(JsValue::from("Failed creating transaction"));
//     }
//     let transaction = transaction.unwrap();
//     let wasm_transaction = WasmTransaction::from_transaction(transaction);
//     Ok(wasm_transaction)
// }

pub async fn get_latest_block_hash() -> String {
    debug!("get_latest_block_hash");
    let saito = SAITO.lock().await;
    let blockchain = saito.as_ref().unwrap().context.blockchain_lock.read().await;
    let hash = blockchain.get_latest_block_hash();

    hash.to_hex().into()
}

// pub async fn get_block(block_hash: String) -> Result<WasmBlock, JsValue> {
//     let block_hash = string_to_hex(block_hash).or(Err(JsValue::from(
//         "Failed parsing block hash string to key",
//     )))?;
//
//     let saito = SAITO.lock().await;
//     let blockchain = saito
//         .as_ref()
//         .unwrap()
//         .routing_thread
//         .blockchain_lock
//         .read()
//         .await;
//
//     let result = blockchain.get_block(&block_hash);
//
//     if result.is_none() {
//         warn!("block {:?} not found", block_hash.to_hex());
//         return Err(JsValue::from("block not found"));
//     }
//     let block = result.cloned().unwrap();
//
//     Ok(WasmBlock::from_block(block))
// }

pub async fn process_new_peer(peer_index: PeerIndex) {
    debug!("process_new_peer : {:?}", peer_index);
    let mut saito = SAITO.lock().await;

    saito
        .as_mut()
        .unwrap()
        .routing_thread
        .process_network_event(NetworkEvent::PeerConnectionResult {
            result: Ok(peer_index),
        })
        .await;
}

// pub async fn process_stun_peer(peer_index: PeerIndex, public_key: String) -> Result<(), JsValue> {
//     debug!(
//         "processing stun peer with index: {:?} and public key: {:?} ",
//         peer_index, public_key
//     );
//     let mut saito = SAITO.lock().await;
//     let key: SaitoPublicKey = string_to_key(public_key.into())
//         .map_err(|e| JsValue::from_str(&format!("Failed to parse public key: {}", e)))?;
//
//     saito
//         .as_mut()
//         .unwrap()
//         .routing_thread
//         .process_network_event(NetworkEvent::AddStunPeer {
//             peer_index,
//             public_key: key,
//         })
//         .await;
//     Ok(())
// }

pub async fn remove_stun_peer(peer_index: PeerIndex) {
    debug!(
        "removing stun peer with index: {:?} from netowrk ",
        peer_index
    );
    let mut saito = SAITO.lock().await;
    saito
        .as_mut()
        .unwrap()
        .routing_thread
        .process_network_event(NetworkEvent::RemoveStunPeer { peer_index })
        .await;
}

pub async fn get_next_peer_index() -> PeerIndex {
    let mut saito = SAITO.lock().await;
    let mut peers = saito
        .as_mut()
        .unwrap()
        .routing_thread
        .network
        .peer_lock
        .write()
        .await;

    peers.peer_counter.get_next_index()
}

pub async fn process_peer_disconnection(peer_index: u64) {
    debug!("process_peer_disconnection : {:?}", peer_index);
    let mut saito = SAITO.lock().await;
    saito
        .as_mut()
        .unwrap()
        .routing_thread
        .process_network_event(NetworkEvent::PeerDisconnected {
            peer_index,
            disconnect_type: PeerDisconnectType::ExternalDisconnect,
        })
        .await;
}

// pub async fn process_msg_buffer_from_peer(buffer: js_sys::Uint8Array, peer_index: u64) {
//     let mut saito = SAITO.lock().await;
//     let buffer = buffer.to_vec();
//
//     saito
//         .as_mut()
//         .unwrap()
//         .routing_thread
//         .process_network_event(NetworkEvent::IncomingNetworkMessage { peer_index, buffer })
//         .await;
// }
//
// pub async fn process_fetched_block(
//     buffer: js_sys::Uint8Array,
//     hash: js_sys::Uint8Array,
//     block_id: BlockId,
//     peer_index: PeerIndex,
// ) {
//     let mut saito = SAITO.lock().await;
//     saito
//         .as_mut()
//         .unwrap()
//         .routing_thread
//         .process_network_event(NetworkEvent::BlockFetched {
//             block_hash: hash.to_vec().try_into().unwrap(),
//             block_id,
//             peer_index,
//             buffer: buffer.to_vec(),
//         })
//         .await;
// }
//
// pub async fn process_failed_block_fetch(hash: js_sys::Uint8Array, block_id: u64, peer_index: u64) {
//     let mut saito = SAITO.lock().await;
//     saito
//         .as_mut()
//         .unwrap()
//         .routing_thread
//         .process_network_event(NetworkEvent::BlockFetchFailed {
//             block_hash: hash.to_vec().try_into().unwrap(),
//             peer_index,
//             block_id,
//         })
//         .await;
// }

pub async fn process_timer_event(duration_in_ms: u64) {
    let mut saito = SAITO.lock().await;

    let duration = Duration::from_millis(duration_in_ms);
    const EVENT_LIMIT: u32 = 100;
    let mut event_counter = 0;

    while let Ok(event) = saito.as_mut().unwrap().receiver_for_router.try_recv() {
        let _result = saito
            .as_mut()
            .unwrap()
            .routing_thread
            .process_event(event)
            .await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }

    saito
        .as_mut()
        .unwrap()
        .routing_thread
        .process_timer_event(duration)
        .await;

    event_counter = 0;
    while let Ok(event) = saito.as_mut().unwrap().receiver_for_consensus.try_recv() {
        let _result = saito
            .as_mut()
            .unwrap()
            .consensus_thread
            .process_event(event)
            .await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }

    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .process_timer_event(duration)
        .await;

    event_counter = 0;
    while let Ok(event) = saito.as_mut().unwrap().receiver_for_verification.try_recv() {
        let _result = saito
            .as_mut()
            .unwrap()
            .verification_thread
            .process_event(event)
            .await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }

    saito
        .as_mut()
        .unwrap()
        .verification_thread
        .process_timer_event(duration)
        .await;

    event_counter = 0;
    while let Ok(event) = saito.as_mut().unwrap().receiver_for_miner.try_recv() {
        let _result = saito
            .as_mut()
            .unwrap()
            .mining_thread
            .process_event(event)
            .await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }

    saito
        .as_mut()
        .unwrap()
        .mining_thread
        .process_timer_event(duration)
        .await;

    saito
        .as_mut()
        .unwrap()
        .stat_thread
        .process_timer_event(duration)
        .await;

    event_counter = 0;
    while let Ok(event) = saito.as_mut().unwrap().receiver_for_stats.try_recv() {
        let _result = saito
            .as_mut()
            .unwrap()
            .stat_thread
            .process_event(event)
            .await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }
}

pub async fn process_stat_interval(current_time: Timestamp) {
    let mut saito = SAITO.lock().await;

    saito
        .as_mut()
        .unwrap()
        .routing_thread
        .on_stat_interval(current_time)
        .await;

    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .on_stat_interval(current_time)
        .await;

    saito
        .as_mut()
        .unwrap()
        .verification_thread
        .on_stat_interval(current_time)
        .await;

    saito
        .as_mut()
        .unwrap()
        .mining_thread
        .on_stat_interval(current_time)
        .await;
}

pub async fn get_peer(peer_index: u64) -> Option<WasmPeer> {
    let saito = SAITO.lock().await;
    let peers = saito
        .as_ref()
        .unwrap()
        .routing_thread
        .network
        .peer_lock
        .read()
        .await;
    let peer = peers.find_peer_by_index(peer_index);
    if peer.is_none() || peer.unwrap().get_public_key().is_none() {
        warn!("peer not found");
        return None;
    }
    let peer = peer.cloned().unwrap();
    Some(WasmPeer::new_from_peer(peer))
}

pub async fn update_from_balance_snapshot(snapshot: WasmBalanceSnapshot) {
    let saito = SAITO.lock().await;
    let mut wallet = saito
        .as_ref()
        .unwrap()
        .routing_thread
        .wallet_lock
        .write()
        .await;
    wallet.update_from_balance_snapshot(
        snapshot.get_snapshot(),
        Some(&saito.as_ref().unwrap().routing_thread.network),
    );
}

pub fn generate_private_key() -> String {
    info!("generate_private_key");
    let (_, private_key) = generate_keys_wasm();
    private_key.to_hex().into()
}

pub async fn propagate_transaction(tx: &WasmTransaction) {
    trace!("propagate_transaction");

    let mut saito = SAITO.lock().await;
    let mut tx = tx.clone().tx;
    {
        let wallet = saito
            .as_ref()
            .unwrap()
            .routing_thread
            .wallet_lock
            .read()
            .await;
        tx.generate(&wallet.public_key, 0, 0);
    }
    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .process_event(ConsensusEvent::NewTransaction { transaction: tx })
        .await;
}
//
// pub async fn get_wallet() -> WasmWallet {
//     let saito = SAITO.lock().await;
//     return saito.as_ref().unwrap().wallet.clone();
// }
//
// pub async fn get_blockchain() -> WasmBlockchain {
//     let saito = SAITO.lock().await;
//     saito.as_ref().unwrap().blockchain.clone()
// }
//
// pub async fn get_mempool_txs() -> js_sys::Array {
//     let saito = SAITO.lock().await;
//     let mempool = saito
//         .as_ref()
//         .unwrap()
//         .consensus_thread
//         .mempool_lock
//         .read()
//         .await;
//     let txs = js_sys::Array::new_with_length(mempool.transactions.len() as u32);
//     for (index, (_, tx)) in mempool.transactions.iter().enumerate() {
//         let wasm_tx = WasmTransaction::from_transaction(tx.clone());
//         txs.set(index as u32, JsValue::from(wasm_tx));
//     }
//
//     txs
// }
//
// pub async fn set_wallet_version(major: u8, minor: u8, patch: u16) {
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.as_ref().unwrap().wallet.wallet.write().await;
//     wallet.wallet_version = Version {
//         major,
//         minor,
//         patch,
//     };
// }

pub fn is_valid_public_key(key: String) -> bool {
    let result = string_to_key(key);
    if result.is_err() {
        return false;
    }
    let key: SaitoPublicKey = result.unwrap();
    saito_core::core::util::crypto::is_valid_public_key(&key)
}

pub async fn write_issuance_file(threshold: Currency) {
    let mut saito = SAITO.lock().await;
    let _configs_lock = saito.as_mut().unwrap().routing_thread.config_lock.clone();
    let _mempool_lock = saito.as_mut().unwrap().routing_thread.mempool_lock.clone();
    let blockchain_lock = saito
        .as_mut()
        .unwrap()
        .routing_thread
        .blockchain_lock
        .clone();
    let storage = &mut saito.as_mut().unwrap().consensus_thread.storage;
    let _list = storage.load_block_name_list().await.unwrap();

    let blockchain = blockchain_lock.write().await;

    info!("utxo size : {:?}", blockchain.utxoset.len());

    let data = blockchain.get_utxoset_data();

    info!("{:?} entries in utxo to write to file", data.len());
    let issuance_path: String = "./data/issuance.file".to_string();
    info!("opening file : {:?}", issuance_path);

    let mut buffer: Vec<u8> = vec![];
    let slip_type = "Normal";
    let mut aggregated_value = 0;
    let mut total_written_lines = 0;
    for (key, value) in &data {
        if value < &threshold {
            aggregated_value += value;
        } else {
            total_written_lines += 1;
            let key_base58 = key.to_base58();

            let s = format!("{}\t{}\t{}\n", value, key_base58, slip_type);
            let buf = s.as_bytes();
            buffer.extend(buf);
        };
    }

    // add remaining value
    if aggregated_value > 0 {
        total_written_lines += 1;
        let s = format!(
            "{}\t{}\t{}\n",
            aggregated_value,
            PROJECT_PUBLIC_KEY.to_string(),
            slip_type
        );
        let buf = s.as_bytes();
        buffer.extend(buf);
    }

    storage
        .io_interface
        .write_value(issuance_path.as_str(), buffer.as_slice())
        .await
        .expect("issuance file should be written");

    info!("total written lines : {:?}", total_written_lines);
}

pub async fn disable_bundling_blocks_by_timer() {
    let mut saito = SAITO.lock().await;
    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .produce_blocks_by_timer = false;
}
pub async fn produce_block_with_gt() {
    let mut saito = SAITO.lock().await;

    {
        let miner = &mut saito.as_mut().unwrap().mining_thread;
        info!("mining for a gt...");
        loop {
            if let Some(gt) = miner.mine().await {
                info!("gt found");
                saito
                    .as_mut()
                    .unwrap()
                    .consensus_thread
                    .add_gt_to_mempool(gt)
                    .await;
                break;
            }
        }
    }

    let timestamp = saito
        .as_ref()
        .unwrap()
        .consensus_thread
        .timer
        .get_timestamp_in_ms();
    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .produce_block(timestamp)
        .await;
}

pub async fn produce_block_without_gt() {
    let mut saito = SAITO.lock().await;
    let timestamp = saito
        .as_ref()
        .unwrap()
        .consensus_thread
        .timer
        .get_timestamp_in_ms();
    saito
        .as_mut()
        .unwrap()
        .consensus_thread
        .produce_block(timestamp)
        .await;
}

pub fn generate_keys_wasm() -> (SaitoPublicKey, SaitoPrivateKey) {
    let (mut secret_key, mut public_key) =
        SECP256K1.generate_keypair(&mut rand::rngs::OsRng::default());
    while public_key.serialize().to_base58().len() != 44 {
        // sometimes secp256k1 address is too big to store in 44 base-58 digits
        let keypair_tuple = SECP256K1.generate_keypair(&mut rand::rngs::OsRng::default());
        secret_key = keypair_tuple.0;
        public_key = keypair_tuple.1;
    }
    let mut secret_bytes = [0u8; 32];
    for i in 0..32 {
        secret_bytes[i] = secret_key[i];
    }
    (public_key.serialize(), secret_bytes)
}

pub fn string_to_key<T: TryFrom<Vec<u8>> + PrintForLog<T>>(key: String) -> Result<T, Error>
where
    <T as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    let str = key;

    if str.is_empty() {
        // debug!("cannot convert empty string to key");
        return Err(Error::from(ErrorKind::InvalidInput));
    }

    let key = T::from_base58(str.as_str());
    if key.is_err() {
        // error!(
        //     "failed parsing key : {:?}. str : {:?}",
        //     key.err().unwrap(),
        //     str
        // );
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let key = key.unwrap();
    Ok(key)
}

pub fn string_to_hex<T: TryFrom<Vec<u8>> + PrintForLog<T>>(key: String) -> Result<T, Error>
where
    <T as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    let str = key;
    if str.is_empty() {
        debug!("cannot convert empty string to hex");
        return Err(Error::from(ErrorKind::InvalidInput));
    }

    let key = T::from_hex(str.as_str());
    if key.is_err() {
        error!(
            "failed parsing hex : {:?}. str : {:?}",
            key.err().unwrap(),
            str
        );
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let key = key.unwrap();
    Ok(key)
}
//
// pub fn string_array_to_base58_keys<T: TryFrom<Vec<u8>> + PrintForLog<T>>(
//     array: js_sys::Array,
// ) -> Vec<T> {
//     let array: Vec<T> = array
//         .to_vec()
//         .drain(..)
//         .filter_map(|key| {
//             let key: String = key.as_string()?;
//             let key = T::from_base58(key.as_str());
//             if key.is_err() {
//                 return None;
//             }
//             let key: T = key.unwrap();
//             Some(key)
//         })
//         .collect();
//     array
// }

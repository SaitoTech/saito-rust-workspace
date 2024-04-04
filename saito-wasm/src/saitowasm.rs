// extern crate wee_alloc;
//
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::time::Duration;

use figment::providers::{Format, Json};
use figment::Figment;
use js_sys::{Array, BigInt, JsString, Uint8Array};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use secp256k1::SECP256K1;
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};
use wasm_bindgen::prelude::*;

use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::consensus::blockchain_sync_state::BlockchainSyncState;
use saito_core::core::consensus::context::Context;
use saito_core::core::consensus::mempool::Mempool;
use saito_core::core::consensus::peer_collection::PeerCollection;
use saito_core::core::consensus::transaction::Transaction;
use saito_core::core::consensus::wallet::Wallet;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::defs::{
    Currency, PeerIndex, PrintForLog, SaitoPrivateKey, SaitoPublicKey, StatVariable, STAT_BIN_COUNT,
};
use saito_core::core::io::network::Network;
use saito_core::core::io::network_event::NetworkEvent;
use saito_core::core::io::storage::Storage;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::msg::api_message::ApiMessage;
use saito_core::core::msg::message::Message;
use saito_core::core::process::process_event::ProcessEvent;
use saito_core::core::process::version::Version;
use saito_core::core::routing_thread::{RoutingEvent, RoutingStats, RoutingThread};
use saito_core::core::util::configuration::{Configuration, PeerConfig};
use saito_core::core::util::crypto::{generate_keypair_from_private_key, sign};
use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};

use crate::wasm_balance_snapshot::WasmBalanceSnapshot;
use crate::wasm_block::WasmBlock;
use crate::wasm_blockchain::WasmBlockchain;
use crate::wasm_configuration::WasmConfiguration;
use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_peer::WasmPeer;
use crate::wasm_slip::WasmSlip;
use crate::wasm_time_keeper::WasmTimeKeeper;
use crate::wasm_transaction::WasmTransaction;
use crate::wasm_wallet::WasmWallet;

#[wasm_bindgen]
pub struct SaitoWasm {
    pub(crate) routing_thread: RoutingThread,
    consensus_thread: ConsensusThread,
    mining_thread: MiningThread,
    verification_thread: VerificationThread,
    receiver_for_router: Receiver<RoutingEvent>,
    receiver_for_consensus: Receiver<ConsensusEvent>,
    receiver_for_miner: Receiver<MiningEvent>,
    receiver_for_verification: Receiver<VerifyRequest>,
    pub(crate) context: Context,
    wallet: WasmWallet,
    blockchain: WasmBlockchain,
}

lazy_static! {
    pub static ref SAITO: Mutex<SaitoWasm> = Mutex::new(new());
    static ref CONFIGS: Arc<RwLock<dyn Configuration + Send + Sync>> =
        Arc::new(RwLock::new(WasmConfiguration::new()));
    static ref PRIVATE_KEY: Mutex<String> = Mutex::new("".to_string());
}

// #[wasm_bindgen]
// impl SaitoWasm {}

pub fn new() -> SaitoWasm {
    info!("creating new saito wasm instance");

    // let keys = generate_keys_wasm();
    // let private_key;
    // let public_key;
    // {
    //     let key = PRIVATE_KEY.lock().await;
    //     private_key = key;
    //
    // }
    // let keys = generate_public_key()
    let wallet = Arc::new(RwLock::new(Wallet::new([0; 32], [0; 33])));
    // {
    //     Wallet::load(Box::new(WasmIoHandler {})).await;
    // }
    // let public_key = wallet.public_key.clone();
    // let private_key = wallet.private_key.clone();
    let configuration: Arc<RwLock<dyn Configuration + Send + Sync>> = CONFIGS.clone();

    let channel_size = 1_000_000;
    // {
    //     let configs = configuration.read().await;
    //     channel_size = configs.get_server_configs().unwrap().channel_size;
    // }

    let peers = Arc::new(RwLock::new(PeerCollection::new()));
    let context = Context {
        blockchain_lock: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
        mempool_lock: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
        wallet_lock: wallet.clone(),
        config_lock: configuration.clone(),
    };

    let (sender_to_consensus, receiver_in_mempool) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_blockchain, receiver_in_blockchain) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_stat, _receiver_in_stats) = tokio::sync::mpsc::channel(channel_size);
    let (sender_to_verification, receiver_in_verification) =
        tokio::sync::mpsc::channel(channel_size);

    SaitoWasm {
        routing_thread: RoutingThread {
            blockchain_lock: context.blockchain_lock.clone(),
            mempool_lock: context.mempool_lock.clone(),
            sender_to_consensus: sender_to_consensus.clone(),
            sender_to_miner: sender_to_miner.clone(),
            static_peers: vec![],
            config_lock: context.config_lock.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            wallet_lock: wallet.clone(),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet_lock.clone(),
                context.config_lock.clone(),
                Box::new(WasmTimeKeeper {}),
            ),
            reconnection_timer: 0,
            stats: RoutingStats::new(sender_to_stat.clone()),
            senders_to_verification: vec![sender_to_verification.clone()],
            last_verification_thread_index: 0,
            stat_sender: sender_to_stat.clone(),
            blockchain_sync_state: BlockchainSyncState::new(10),
            initial_connection: false,
            reconnection_wait_time: 10_000,
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
            time_keeper: Box::new(WasmTimeKeeper {}),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet_lock.clone(),
                configuration.clone(),
                Box::new(WasmTimeKeeper {}),
            ),
            storage: Storage::new(Box::new(WasmIoHandler {})),
            stats: ConsensusStats::new(sender_to_stat.clone()),
            txs_for_mempool: vec![],
            stat_sender: sender_to_stat.clone(),
            config_lock: configuration.clone(),
        },
        mining_thread: MiningThread {
            wallet_lock: context.wallet_lock.clone(),
            sender_to_mempool: sender_to_consensus.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            miner_active: false,
            target: [0; 32],
            difficulty: 0,
            public_key: [0; 33],
            mined_golden_tickets: 0,
            stat_sender: sender_to_stat.clone(),
            config_lock: configuration.clone(),
            enabled: true,
            mining_iterations: 1_000,
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
        receiver_for_router: receiver_in_blockchain,
        receiver_for_consensus: receiver_in_mempool,
        receiver_for_miner: receiver_in_miner,
        receiver_for_verification: receiver_in_verification,
        wallet: WasmWallet::new_from(
            context.wallet_lock.clone(),
            Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet_lock.clone(),
                configuration.clone(),
                Box::new(WasmTimeKeeper {}),
            ),
        ),
        blockchain: WasmBlockchain {
            blockchain_lock: context.blockchain_lock.clone(),
        },
        context,
    }
}

#[wasm_bindgen]
pub async fn initialize(
    json: JsString,
    private_key: JsString,
    log_level_num: u8,
) -> Result<JsValue, JsValue> {
    let log_level = match log_level_num {
        0 => log::Level::Error,
        1 => log::Level::Warn,
        2 => log::Level::Info,
        3 => log::Level::Debug,
        4 => log::Level::Trace,
        _ => log::Level::Info,
    };

    console_log::init_with_level(log_level).unwrap();

    trace!("trace test");
    debug!("debug test");
    info!("initializing saito-wasm");
    {
        info!("setting configs...");
        let mut configs = CONFIGS.write().await;
        info!("config lock acquired");

        let str: Result<String, _> = json.try_into();
        if str.is_err() {
            error!(
                "cannot parse json configs string : {:?}",
                str.err().unwrap()
            );
            return Err(JsValue::from("Failed parsing configs string"));
        }
        let config = WasmConfiguration::new_from_json(str.expect("couldn't get string").as_str());

        if config.is_err() {
            error!("failed parsing configs. {:?}", config.err().unwrap());
        } else {
            let config = config.unwrap();
            info!("config : {:?}", config);
            configs.replace(&config);
        }
    }

    let mut saito = SAITO.lock().await;
    let private_key: SaitoPrivateKey = string_to_hex(private_key).or(Err(JsValue::from(
        "Failed parsing private key string to key",
    )))?;
    {
        let mut wallet = saito.context.wallet_lock.write().await;
        if private_key != [0; 32] {
            let keys = generate_keypair_from_private_key(private_key.as_slice());
            wallet.private_key = keys.1;
            wallet.public_key = keys.0;
        }
    }
    saito.mining_thread.on_init().await;
    saito.consensus_thread.on_init().await;
    saito.verification_thread.on_init().await;
    saito.routing_thread.on_init().await;

    return Ok(JsValue::from("initialized"));
}

#[wasm_bindgen]
pub async fn create_transaction(
    public_key: JsString,
    amount: u64,
    fee: u64,
    force_merge: bool,
) -> Result<WasmTransaction, JsValue> {
    trace!("create_transaction : {:?}", public_key.to_string());
    let saito = SAITO.lock().await;
    let mut wallet = saito.context.wallet_lock.write().await;
    let key = string_to_key(public_key).or(Err(JsValue::from(
        "Failed parsing public key string to key",
    )))?;

    let transaction = Transaction::create(
        &mut wallet,
        key,
        amount,
        fee,
        force_merge,
        Some(&saito.consensus_thread.network),
    );
    if transaction.is_err() {
        error!(
            "failed creating transaction. {:?}",
            transaction.err().unwrap()
        );
        return Err(JsValue::from("Failed creating transaction"));
    }
    let transaction = transaction.unwrap();
    let wasm_transaction = WasmTransaction::from_transaction(transaction);
    Ok(wasm_transaction)
}

#[wasm_bindgen]
pub async fn create_transaction_with_multiple_payments(
    public_keys: js_sys::Array,
    amounts: js_sys::BigUint64Array,
    fee: u64,
    force_merge: bool,
) -> Result<WasmTransaction, JsValue> {
    let saito = SAITO.lock().await;
    let mut wallet = saito.context.wallet_lock.write().await;

    let keys: Vec<SaitoPublicKey> = string_array_to_base58_keys(public_keys);
    let amounts: Vec<Currency> = amounts.to_vec();

    if keys.len() != amounts.len() {
        return Err(JsValue::from("keys and payments have different counts"));
    }

    let transaction = Transaction::create_with_multiple_payments(
        &mut wallet,
        keys,
        amounts,
        fee,
        Some(&saito.consensus_thread.network),
    );
    if transaction.is_err() {
        error!(
            "failed creating transaction. {:?}",
            transaction.err().unwrap()
        );
        return Err(JsValue::from("Failed creating transaction"));
    }
    let transaction = transaction.unwrap();
    let wasm_transaction = WasmTransaction::from_transaction(transaction);
    Ok(wasm_transaction)
}

#[wasm_bindgen]
pub async fn get_latest_block_hash() -> JsString {
    debug!("get_latest_block_hash");
    let saito = SAITO.lock().await;
    let blockchain = saito.context.blockchain_lock.read().await;
    let hash = blockchain.get_latest_block_hash();

    hash.to_hex().into()
}

#[wasm_bindgen]
pub async fn get_block(block_hash: JsString) -> Result<WasmBlock, JsValue> {
    // debug!("get_block");
    let block_hash = string_to_hex(block_hash).or(Err(JsValue::from(
        "Failed parsing block hash string to key",
    )))?;

    let saito = SAITO.lock().await;
    let blockchain = saito.routing_thread.blockchain_lock.read().await;

    let result = blockchain.get_block(&block_hash);

    if result.is_none() {
        warn!("block {:?} not found", block_hash.to_hex());
        return Err(JsValue::from("block not found"));
    }
    let block = result.cloned().unwrap();

    Ok(WasmBlock::from_block(block))
}

#[wasm_bindgen]
pub async fn process_new_peer(index: u64, peer_config: JsValue) {
    debug!("process_new_peer : {:?}", index);
    let mut saito = SAITO.lock().await;

    let mut peer_details = None;
    if peer_config.is_truthy() {
        let result = js_sys::JSON::stringify(&peer_config);
        if result.is_err() {
            error!("failed processing new peer. failed parsing json info");
            error!("{:?}", result.err().unwrap());
            return;
        }
        let json = result.unwrap();

        let configs = Figment::new()
            .merge(Json::string(json.as_string().unwrap().as_str()))
            .extract::<PeerConfig>();
        if configs.is_err() {
            error!(
                "failed parsing json string to configs. {:?}",
                configs.err().unwrap()
            );
            return;
        }
        let configs = configs.unwrap();
        peer_details = Some(configs);
    }

    saito
        .routing_thread
        .process_network_event(NetworkEvent::PeerConnectionResult {
            peer_details,
            result: Ok(index),
        })
        .await;
}

#[wasm_bindgen]
pub async fn process_peer_disconnection(peer_index: u64) {
    debug!("process_peer_disconnection : {:?}", peer_index);
    let mut saito = SAITO.lock().await;
    saito
        .routing_thread
        .process_network_event(NetworkEvent::PeerDisconnected { peer_index })
        .await;
}

#[wasm_bindgen]
pub async fn process_msg_buffer_from_peer(buffer: js_sys::Uint8Array, peer_index: u64) {
    let mut saito = SAITO.lock().await;
    let buffer = buffer.to_vec();

    saito
        .routing_thread
        .process_network_event(NetworkEvent::IncomingNetworkMessage { peer_index, buffer })
        .await;
}

#[wasm_bindgen]
pub async fn process_fetched_block(
    buffer: js_sys::Uint8Array,
    hash: js_sys::Uint8Array,
    peer_index: u64,
) {
    let mut saito = SAITO.lock().await;
    saito
        .routing_thread
        .process_network_event(NetworkEvent::BlockFetched {
            block_hash: hash.to_vec().try_into().unwrap(),
            peer_index,
            buffer: buffer.to_vec(),
        })
        .await;
}

#[wasm_bindgen]
pub async fn process_failed_block_fetch(hash: js_sys::Uint8Array, block_id: u64, peer_index: u64) {
    let mut saito = SAITO.lock().await;
    saito
        .routing_thread
        .process_network_event(NetworkEvent::BlockFetchFailed {
            block_hash: hash.to_vec().try_into().unwrap(),
            peer_index,
            block_id,
        })
        .await;
}

#[wasm_bindgen]
pub async fn process_timer_event(duration_in_ms: u64) {
    let mut saito = SAITO.lock().await;

    let duration = Duration::from_millis(duration_in_ms);
    const EVENT_LIMIT: u32 = 100;
    let mut event_counter = 0;

    while let Ok(event) = saito.receiver_for_router.try_recv() {
        let _result = saito.routing_thread.process_event(event).await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }
    event_counter = 0;

    saito.routing_thread.process_timer_event(duration).await;

    while let Ok(event) = saito.receiver_for_consensus.try_recv() {
        let _result = saito.consensus_thread.process_event(event).await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }
    event_counter = 0;

    saito.consensus_thread.process_timer_event(duration).await;

    while let Ok(event) = saito.receiver_for_verification.try_recv() {
        let _result = saito.verification_thread.process_event(event).await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }
    event_counter = 0;

    saito
        .verification_thread
        .process_timer_event(duration.clone())
        .await;

    while let Ok(event) = saito.receiver_for_miner.try_recv() {
        let _result = saito.mining_thread.process_event(event).await;
        event_counter += 1;
        if event_counter >= EVENT_LIMIT {
            break;
        }
    }
    event_counter = 0;

    saito.mining_thread.process_timer_event(duration).await;
}

#[wasm_bindgen]
pub fn hash(buffer: Uint8Array) -> JsString {
    let buffer: Vec<u8> = buffer.to_vec();
    let hash = saito_core::core::util::crypto::hash(&buffer);
    let str = hash.to_hex();
    let str: js_sys::JsString = str.into();
    str
}

#[wasm_bindgen]
pub fn sign_buffer(buffer: Uint8Array, private_key: JsString) -> Result<JsString, JsValue> {
    let buffer = buffer.to_vec();
    let key = string_to_hex(private_key).or(Err(JsValue::from(
        "Failed parsing private key string to key",
    )))?;
    let result = sign(&buffer, &key);

    let signature = result.to_hex();
    Ok(signature.into())
}

#[wasm_bindgen]
pub fn verify_signature(buffer: Uint8Array, signature: JsString, public_key: JsString) -> bool {
    let sig = string_to_hex(signature);
    if sig.is_err() {
        error!("signature is invalid");
        return false;
    }
    let sig = sig.unwrap();
    let key = string_to_key(public_key);
    if key.is_err() {
        error!(
            "failed parsing public key from string. {:?}",
            key.err().unwrap()
        );
        return false;
    }
    let buffer = buffer.to_vec();
    let h = saito_core::core::util::crypto::hash(&buffer);
    saito_core::core::util::crypto::verify_signature(&h, &sig, &key.unwrap())
}

#[wasm_bindgen]
pub async fn get_peers() -> Array {
    let saito = SAITO.lock().await;
    let peers = saito.routing_thread.network.peer_lock.read().await;
    let valid_peer_count = peers
        .index_to_peers
        .iter()
        .filter(|(_index, peer)| peer.public_key.is_some())
        .count();
    let array = Array::new_with_length(valid_peer_count as u32);
    let mut array_index = 0;
    for (_i, (_peer_index, peer)) in peers.index_to_peers.iter().enumerate() {
        if peer.public_key.is_none() {
            continue;
        }
        let peer = peer.clone();
        array.set(
            array_index as u32,
            JsValue::from(WasmPeer::new_from_peer(peer)),
        );
        array_index += 1;
    }
    array
}

#[wasm_bindgen]
pub async fn get_peer(peer_index: u64) -> Option<WasmPeer> {
    let saito = SAITO.lock().await;
    let peers = saito.routing_thread.network.peer_lock.read().await;
    let peer = peers.find_peer_by_index(peer_index);
    if peer.is_none() || peer.unwrap().public_key.is_none() {
        warn!("peer not found");
        return None;
    }
    let peer = peer.cloned().unwrap();
    Some(WasmPeer::new_from_peer(peer))
}

#[wasm_bindgen]
pub async fn get_account_slips(public_key: JsString) -> Result<Array, JsValue> {
    let saito = SAITO.lock().await;
    let blockchain = saito.routing_thread.blockchain_lock.read().await;
    let key = string_to_key(public_key).or(Err(JsValue::from(
        "Failed parsing public key string to key",
    )))?;
    let mut slips = blockchain.get_slips_for(key);
    let array = js_sys::Array::new_with_length(slips.len() as u32);

    for (index, slip) in slips.drain(..).enumerate() {
        let wasm_slip = WasmSlip::new_from_slip(slip);
        array.set(index as u32, JsValue::from(wasm_slip));
    }

    Ok(array)
}

#[wasm_bindgen]
pub async fn get_balance_snapshot(keys: js_sys::Array) -> WasmBalanceSnapshot {
    let keys: Vec<SaitoPublicKey> = string_array_to_base58_keys(keys);

    let saito = SAITO.lock().await;
    let blockchain = saito.routing_thread.blockchain_lock.read().await;
    let snapshot = blockchain.get_balance_snapshot(keys);

    WasmBalanceSnapshot::new(snapshot)
}

#[wasm_bindgen]
pub async fn update_from_balance_snapshot(snapshot: WasmBalanceSnapshot) {
    let saito = SAITO.lock().await;
    let mut wallet = saito.routing_thread.wallet_lock.write().await;
    wallet
        .update_from_balance_snapshot(snapshot.get_snapshot(), Some(&saito.routing_thread.network));
}

#[wasm_bindgen]
pub fn generate_private_key() -> JsString {
    info!("generate_private_key");
    let (_, private_key) = generate_keys_wasm();
    private_key.to_hex().into()
}

#[wasm_bindgen]
pub fn generate_public_key(private_key: JsString) -> Result<JsString, JsValue> {
    info!("generate_public_key");
    let private_key: SaitoPrivateKey = string_to_hex(private_key).or(Err(JsValue::from(
        "Failed parsing private key string to key",
    )))?;
    let (public_key, _) = generate_keypair_from_private_key(&private_key);
    Ok(public_key.to_base58().into())
}

#[wasm_bindgen]
pub async fn propagate_transaction(tx: &WasmTransaction) {
    trace!("propagate_transaction");

    let mut saito = SAITO.lock().await;
    saito
        .consensus_thread
        .process_event(ConsensusEvent::NewTransaction {
            transaction: tx.tx.clone(),
        })
        .await;
}

#[wasm_bindgen]
pub async fn send_api_call(buffer: Uint8Array, msg_index: u32, peer_index: PeerIndex) {
    trace!("send_api_call : {:?}", peer_index);
    let saito = SAITO.lock().await;
    let api_message = ApiMessage {
        msg_index,
        data: buffer.to_vec(),
    };
    let message = Message::ApplicationMessage(api_message);
    let buffer = message.serialize();
    if peer_index == 0 {
        saito
            .routing_thread
            .network
            .io_interface
            .send_message_to_all(buffer, vec![])
            .await
            .unwrap();
    } else {
        saito
            .routing_thread
            .network
            .io_interface
            .send_message(peer_index, buffer)
            .await
            .unwrap();
    }
}

#[wasm_bindgen]
pub async fn send_api_success(buffer: Uint8Array, msg_index: u32, peer_index: PeerIndex) {
    trace!("send_api_success : {:?}", peer_index);
    let saito = SAITO.lock().await;
    let api_message = ApiMessage {
        msg_index,
        data: buffer.to_vec(),
    };
    let message = Message::Result(api_message);
    let buffer = message.serialize();

    saito
        .routing_thread
        .network
        .io_interface
        .send_message(peer_index, buffer)
        .await
        .unwrap();
}

#[wasm_bindgen]
pub async fn send_api_error(buffer: Uint8Array, msg_index: u32, peer_index: PeerIndex) {
    trace!("send_api_error : {:?}", peer_index);
    let saito = SAITO.lock().await;
    let api_message = ApiMessage {
        msg_index,
        data: buffer.to_vec(),
    };
    let message = Message::Error(api_message);
    let buffer = message.serialize();

    saito
        .routing_thread
        .network
        .io_interface
        .send_message(peer_index, buffer)
        .await
        .unwrap();
}

// #[wasm_bindgen]
// pub async fn propagate_services(peer_index: PeerIndex, services: JsValue) {
//     info!("propagating services : {:?} - {:?}", peer_index, services);
//     let arr = js_sys::Array::from(&services);
//     let mut services: Vec<WasmPeerService> = serde_wasm_bindgen::from_value(services).unwrap();
//     // for i in 0..arr.length() {
//     //     let service = WasmPeerService::from(arr.at(i as i32));
//     //     let service = service.service;
//     //     services.push(service);
//     // }
//     let services = services.drain(..).map(|s| s.service).collect();
//     let saito = SAITO.lock().await;
//     saito
//         .routing_thread
//         .network
//         .propagate_services(peer_index, services)
//         .await;
// }

#[wasm_bindgen]
pub async fn get_wallet() -> WasmWallet {
    let saito = SAITO.lock().await;
    return saito.wallet.clone();
}

#[wasm_bindgen]
pub async fn get_blockchain() -> WasmBlockchain {
    let saito = SAITO.lock().await;
    return saito.blockchain.clone();
}

#[wasm_bindgen]
pub async fn get_mempool_txs() -> js_sys::Array {
    let saito = SAITO.lock().await;
    let mempool = saito.consensus_thread.mempool_lock.read().await;
    let txs = js_sys::Array::new_with_length(mempool.transactions.len() as u32);
    for (index, (_, tx)) in mempool.transactions.iter().enumerate() {
        let wasm_tx = WasmTransaction::from_transaction(tx.clone());
        txs.set(index as u32, JsValue::from(wasm_tx));
    }

    txs
}

#[wasm_bindgen]
pub async fn set_wallet_version(major: u8, minor: u8, patch: u16) {
    let saito = SAITO.lock().await;
    let mut wallet = saito.wallet.wallet.write().await;
    wallet.version = Version {
        major,
        minor,
        patch,
    };
}

#[wasm_bindgen]
pub fn is_valid_public_key(key: JsString) -> bool {
    let result = string_to_key(key);
    if result.is_err() {
        return false;
    }
    let key: SaitoPublicKey = result.unwrap();
    saito_core::core::util::crypto::is_valid_public_key(&key)
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

pub fn string_to_key<T: TryFrom<Vec<u8>> + PrintForLog<T>>(key: JsString) -> Result<T, Error>
where
    <T as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    let str = key.as_string();
    if str.is_none() {
        error!("cannot convert wasm string to rust string");
        return Err(Error::from(ErrorKind::InvalidInput));
    }

    let str = str.unwrap();
    if str.is_empty() {
        debug!("cannot convert empty string to key");
        return Err(Error::from(ErrorKind::InvalidInput));
    }

    let key = T::from_base58(str.as_str());
    if key.is_err() {
        error!(
            "failed parsing key : {:?}. str : {:?}",
            key.err().unwrap(),
            str
        );
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let key = key.unwrap();
    Ok(key)
}

pub fn string_to_hex<T: TryFrom<Vec<u8>> + PrintForLog<T>>(key: JsString) -> Result<T, Error>
where
    <T as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    let str = key.as_string();
    if str.is_none() {
        error!("cannot convert wasm string to rust string");
        return Err(Error::from(ErrorKind::InvalidInput));
    }

    let str = str.unwrap();
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

pub fn string_array_to_base58_keys<T: TryFrom<Vec<u8>> + PrintForLog<T>>(
    array: js_sys::Array,
) -> Vec<T> {
    let array: Vec<T> = array
        .to_vec()
        .drain(..)
        .filter_map(|key| {
            let key: Option<String> = key.as_string();
            if key.is_none() {
                return None;
            }
            let key: String = key.unwrap();
            let key = T::from_base58(key.as_str());
            if key.is_err() {
                return None;
            }
            let key: T = key.unwrap();
            return Some(key);
        })
        .collect();
    array
}

// #[cfg(test)]
// mod test {
//     use js_sys::JsString;
//     use saito_core::common::defs::SaitoPublicKey;
//
//     use crate::saitowasm::string_to_key;
//
//     #[test]
//     fn string_to_key_test() {
//         let empty_key = [
//             0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
//             0, 0, 0, 0,
//         ];
//         let key = string_to_key(JsString::from(""));
//         assert!(key.is_ok());
//         let key: SaitoPublicKey = key.unwrap();
//         assert_eq!(key, empty_key);
//     }
// }

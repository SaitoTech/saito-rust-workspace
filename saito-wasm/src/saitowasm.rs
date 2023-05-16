// extern crate wee_alloc;
//
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::time::Duration;

use base58::ToBase58;
use figment::providers::{Format, Json};
use figment::Figment;
use js_sys::{Array, JsString, Uint8Array};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn, Level};
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};
use wasm_bindgen::prelude::*;

use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::{
    Currency, PeerIndex, SaitoPrivateKey, SaitoPublicKey, StatVariable, STAT_BIN_COUNT,
};
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::blockchain_sync_state::BlockchainSyncState;
use saito_core::core::data::configuration::{Configuration, PeerConfig};
use saito_core::core::data::context::Context;
use saito_core::core::data::crypto::{generate_keypair_from_private_key, SECP256K1};
use saito_core::core::data::crypto::{hash as hash_fn, sign};
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::msg::api_message::ApiMessage;
use saito_core::core::data::msg::message::Message;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::{MiningEvent, MiningThread};
use saito_core::core::routing_thread::{RoutingEvent, RoutingStats, RoutingThread};
use saito_core::core::verification_thread::{VerificationThread, VerifyRequest};

use crate::wasm_block::WasmBlock;
use crate::wasm_blockchain::WasmBlockchain;
use crate::wasm_configuration::WasmConfiguration;
use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_peer::WasmPeer;
use crate::wasm_time_keeper::WasmTimeKeeper;
use crate::wasm_transaction::WasmTransaction;
use crate::wasm_wallet::WasmWallet;

// pub(crate) struct NetworkResultFuture {
//     pub result: Option<Result<Vec<u8>, Error>>,
//     pub key: u64,
// }
//
// // TODO : check if this gets called from somewhere or need a runtime
// impl Future for NetworkResultFuture {
//     type Output = Result<Vec<u8>, Error>;
//
//     fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
//         let mut saito = SAITO.blocking_lock();
//         let result = saito.results.remove(&self.key);
//         if result.is_some() {
//             let result = result.unwrap();
//             return Poll::Ready(result);
//         }
//         let waker = cx.waker().clone();
//         saito.wakers.insert(self.key, waker);
//         return Poll::Pending;
//     }
// }

#[wasm_bindgen]
pub struct SaitoWasm {
    routing_thread: RoutingThread,
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
    let mut wallet = Arc::new(RwLock::new(Wallet::new([0; 32], [0; 33])));
    // {
    //     Wallet::load(Box::new(WasmIoHandler {})).await;
    // }
    // let public_key = wallet.public_key.clone();
    // let private_key = wallet.private_key.clone();
    let configuration: Arc<RwLock<dyn Configuration + Send + Sync>> = CONFIGS.clone();

    let peers = Arc::new(RwLock::new(PeerCollection::new()));
    let context = Context {
        blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
        mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
        wallet: wallet.clone(),
        configuration: configuration.clone(),
    };

    let (sender_to_consensus, receiver_in_mempool) = tokio::sync::mpsc::channel(10);
    let (sender_to_blockchain, receiver_in_blockchain) = tokio::sync::mpsc::channel(10);
    let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(10);
    let (sender_to_stat, _receiver_in_stats) = tokio::sync::mpsc::channel(10);
    let (sender_to_verification, receiver_in_verification) = tokio::sync::mpsc::channel(10);

    SaitoWasm {
        routing_thread: RoutingThread {
            blockchain: context.blockchain.clone(),
            sender_to_consensus: sender_to_consensus.clone(),
            sender_to_miner: sender_to_miner.clone(),
            static_peers: vec![],
            configs: context.configuration.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            wallet: wallet.clone(),
            network: Network::new(
                Box::new(WasmIoHandler {}),
                peers.clone(),
                context.wallet.clone(),
                context.configuration.clone(),
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
                configuration.clone(),
            ),
            storage: Storage::new(Box::new(WasmIoHandler {})),
            stats: ConsensusStats::new(sender_to_stat.clone()),
            txs_for_mempool: vec![],
            stat_sender: sender_to_stat.clone(),
            configs: configuration.clone(),
        },
        mining_thread: MiningThread {
            wallet: context.wallet.clone(),

            sender_to_mempool: sender_to_consensus.clone(),
            time_keeper: Box::new(WasmTimeKeeper {}),
            miner_active: false,
            target: [0; 32],
            difficulty: 0,
            public_key: [0; 33],
            mined_golden_tickets: 0,
            stat_sender: sender_to_stat.clone(),
            configs: configuration,
            enabled: true,
        },
        verification_thread: VerificationThread {
            sender_to_consensus: sender_to_consensus.clone(),
            blockchain: context.blockchain.clone(),
            peers,
            wallet,
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
        wallet: WasmWallet::new_from(context.wallet.clone()),
        blockchain: WasmBlockchain {
            blockchain: context.blockchain.clone(),
        },
        context,
    }
}

#[wasm_bindgen]
pub async fn initialize(json: JsString, private_key: JsString) -> Result<JsValue, JsValue> {
    console_log::init_with_level(Level::Trace).unwrap();

    info!("initializing saito-wasm");
    trace!("trace test");
    debug!("debug test");
    // return Ok(JsValue::from("sss"));
    {
        info!("setting configs...");
        let mut configs = CONFIGS.write().await;
        info!("config lock acquired");
        // let str = js_sys::JSON::stringify(&json);
        // info!("setting configs : {:?}", str.as_string().unwrap());

        let str: Result<String, _> = json.try_into();
        if str.is_err() {
            info!(
                "cannot parse json configs string : {:?}",
                str.err().unwrap()
            );
            return Ok(JsValue::from("initialized"));
        }
        // let str: JsString = str.unwrap();
        let config = WasmConfiguration::new_from_json(str.expect("couldn't get string").as_str());

        if config.is_err() {
            info!("failed parsing configs");
            return Ok(JsValue::from("initialized"));
        }
        let config = config.unwrap();

        // info!("config : {:?}", config);

        configs.replace(&config);
    }

    let mut saito = SAITO.lock().await;
    let private_key: SaitoPrivateKey = string_to_key(private_key).unwrap();
    {
        let mut wallet = saito.context.wallet.write().await;
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
    debug!("create_transaction");
    let saito = SAITO.lock().await;
    let mut wallet = saito.context.wallet.write().await;
    // let (mut wallet, _wallet_) = lock_for_write!(saito.context.wallet, LOCK_ORDER_WALLET);
    let key = string_to_key(public_key);
    if key.is_err() {
        error!("failed parsing public key : {:?}", key.err().unwrap());
        todo!()
    }
    let mut transaction = Transaction::create(&mut wallet, key.unwrap(), amount, fee, force_merge);
    let wasm_transaction = WasmTransaction::from_transaction(transaction);
    return Ok(wasm_transaction);
}

#[wasm_bindgen]
pub async fn get_latest_block_hash() -> JsString {
    debug!("get_latest_block_hash");
    let saito = SAITO.lock().await;
    let blockchain = saito.context.blockchain.read().await;
    let hash = blockchain.get_latest_block_hash();

    hex::encode(hash).into()
}

pub async fn get_block(block_hash: JsString) -> Result<WasmBlock, JsValue> {
    debug!("get_block");
    let block_hash = string_to_key(block_hash);
    if block_hash.is_err() {
        error!("block hash string is invalid");
        todo!()
    }

    let block_hash = block_hash.unwrap();

    let saito = SAITO.lock().await;
    let blockchain = saito.routing_thread.blockchain.read().await;

    let result = blockchain.get_block(&block_hash);

    if result.is_none() {
        warn!("block {:?} not found", hex::encode(block_hash));
        todo!()
    }
    let block = result.cloned().unwrap();

    Ok(WasmBlock::from_block(block))
}

#[wasm_bindgen]
pub async fn process_new_peer(index: u64, peer_config: JsValue) {
    info!("process_new_peer : {:?}", index);
    let mut saito = SAITO.lock().await;

    let mut peer_details = None;
    if peer_config.is_truthy() {
        let result = js_sys::JSON::stringify(&peer_config);
        if result.is_err() {
            error!("failed processing new peer. failed parsing json info");
            error!("{:?}", result.err().unwrap());
            return;
        }
        // drop(peer_config);
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
    info!("process_peer_disconnection : {:?}", peer_index);
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
    info!(
        "process_msg_buffer_from_peer : {:?} length = {:?}",
        peer_index,
        buffer.len()
    );
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
    info!("process_fetched_block : {:?}", peer_index);
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
pub async fn process_timer_event(duration_in_ms: u64) {
    // trace!("process_timer_event");
    let mut saito = SAITO.lock().await;

    let duration = Duration::from_millis(duration_in_ms);

    // blockchain controller
    // TODO : update to recv().await
    let result = saito.receiver_for_router.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let _result = saito.routing_thread.process_event(event).await;
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
        let _result = saito.consensus_thread.process_event(event).await;
    }
    saito
        .consensus_thread
        .process_timer_event(duration.clone())
        .await;

    // verification thread
    let result = saito.receiver_for_verification.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let _result = saito.verification_thread.process_event(event).await;
    }
    saito
        .verification_thread
        .process_timer_event(duration.clone())
        .await;

    // miner controller
    let result = saito.receiver_for_miner.try_recv();
    if result.is_ok() {
        let event = result.unwrap();
        let _result = saito.mining_thread.process_event(event).await;
    }
    saito
        .mining_thread
        .process_timer_event(duration.clone())
        .await;
}

#[wasm_bindgen]
pub fn hash(buffer: Uint8Array) -> JsString {
    let buffer: Vec<u8> = buffer.to_vec();
    let hash = hash_fn(&buffer);
    let str = hex::encode(hash);
    let str: js_sys::JsString = str.into();
    str
}

#[wasm_bindgen]
pub fn sign_buffer(buffer: Uint8Array, private_key: JsString) -> JsString {
    let buffer = buffer.to_vec();
    let key = string_to_key(private_key);
    if key.is_err() {
        error!("key couldn't be parsed : {:?}", key.err().unwrap());
        return "".into();
    }
    let key: SaitoPrivateKey = key.unwrap();

    let result = sign(&buffer, &key);

    let signature = hex::encode(result);
    signature.into()
}

#[wasm_bindgen]
pub fn verify_signature(buffer: Uint8Array, signature: JsString, public_key: JsString) -> bool {
    let sig = string_to_key(signature);
    if sig.is_err() {
        error!("signature is invalid");
        return false;
    }
    let sig = sig.unwrap();
    let key = string_to_key(public_key);
    if key.is_err() {
        error!("key is invalid");
        return false;
    }
    let key = key.unwrap();
    let buffer = buffer.to_vec();
    let h = saito_core::core::data::crypto::hash(&buffer);
    saito_core::core::data::crypto::verify_signature(&h, &sig, &key)
}

#[wasm_bindgen]
pub async fn get_peers() -> Array {
    let saito = SAITO.lock().await;
    let peers = saito.routing_thread.network.peers.read().await;
    let array = Array::new_with_length(peers.index_to_peers.len() as u32);
    for (i, (_peer_index, peer)) in peers.index_to_peers.iter().enumerate() {
        let peer = peer.clone();
        array.set(i as u32, JsValue::from(WasmPeer::new_from_peer(peer)));
    }
    array
}

#[wasm_bindgen]
pub async fn get_peer(peer_index: u64) -> Option<WasmPeer> {
    let saito = SAITO.lock().await;
    let peers = saito.routing_thread.network.peers.read().await;
    let peer = peers.find_peer_by_index(peer_index);
    if peer.is_none() {
        warn!("peer not found");
        // return Err(JsValue::from("peer not found"));
        return None;
        // return Ok(JsValue::NULL);
    }
    let peer = peer.cloned().unwrap();
    Some(WasmPeer::new_from_peer(peer))
}

// #[wasm_bindgen]
// pub async fn get_public_key() -> JsString {
//     let saito = SAITO.lock().await;
//     let wallet = saito.context.wallet.read().await;
//     let key = hex::encode(wallet.public_key);
//     JsString::from(key)
// }
//
// #[wasm_bindgen]
// pub async fn get_private_key() -> JsString {
//     info!("get_private_key");
//     let saito = SAITO.lock().await;
//     let wallet = saito.context.wallet.read().await;
//     let key = hex::encode(wallet.private_key);
//     JsString::from(key)
// }

// #[wasm_bindgen]
// pub async fn get_pending_txs() -> js_sys::Array {
//     let saito = SAITO.lock().await;
//     let wallet = saito.context.wallet.read().await;
//     let array = js_sys::Array::new_with_length(wallet.pending_txs.len() as u32);
//     for (i, tx) in wallet.pending_txs.values().enumerate() {
//         let t = WasmTransaction::from_transaction(tx.clone());
//         array.set(i as u32, JsValue::from(t));
//     }
//     array
// }

// #[wasm_bindgen]
// pub async fn get_balance(_ticker: JsString) -> Currency {
//     info!("get_balance");
//     let saito = SAITO.lock().await;
//     let wallet = saito.context.wallet.read().await;
//     wallet.get_available_balance()
// }

#[wasm_bindgen]
pub fn generate_private_key() -> JsString {
    info!("generate_private_key");
    let (_, private_key) = generate_keys_wasm();
    hex::encode(private_key).into()
}

#[wasm_bindgen]
pub fn generate_public_key(private_key: JsString) -> JsString {
    info!("generate_public_key");
    let private_key: SaitoPrivateKey = string_to_key(private_key).unwrap();
    let (public_key, _) = generate_keypair_from_private_key(&private_key);
    hex::encode(public_key).into()
}

#[wasm_bindgen]
pub async fn propagate_transaction(tx: &WasmTransaction) {
    info!("propagate_transaction");

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
    info!("send_api_call : {:?}", peer_index);
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
    info!("send_api_success : {:?}", peer_index);
    let saito = SAITO.lock().await;
    let api_message = ApiMessage {
        msg_index,
        data: buffer.to_vec(),
    };
    let message = Message::Result(api_message);
    let buffer = message.serialize();

    // let mut tx = Transaction::default();
    // tx.data = buffer;
    // let buffer = Message::Transaction(tx).serialize();
    info!("buffer size = {:?}", buffer.len());
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
    info!("send_api_error : {:?}", peer_index);
    let saito = SAITO.lock().await;
    let api_message = ApiMessage {
        msg_index,
        data: buffer.to_vec(),
    };
    let message = Message::Error(api_message);
    let buffer = message.serialize();

    // let mut tx = Transaction::default();
    // tx.data = buffer;
    // let buffer = Message::Transaction(tx).serialize();

    info!("buffer size = {:?}", buffer.len());
    saito
        .routing_thread
        .network
        .io_interface
        .send_message(peer_index, buffer)
        .await
        .unwrap();
}

#[wasm_bindgen]
pub async fn propagate_services(peer_index: PeerIndex, services: JsValue) {
    info!("propagating services : {:?} - {:?}", peer_index, services);
    let saito = SAITO.lock().await;
    let arr = js_sys::Array::from(&services);
    let mut services = vec![];
    for i in 0..arr.length() {
        let service: String = JsString::from(arr.at(i as i32)).into();
        services.push(service);
    }
    saito
        .routing_thread
        .network
        .propagate_services(peer_index, services)
        .await;
}

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
pub fn test_buffer_in(buffer: js_sys::Uint8Array) {
    let buffer = buffer.to_vec();
}

#[wasm_bindgen]
pub fn test_buffer_out() -> js_sys::Uint8Array {
    let buffer = js_sys::Uint8Array::new_with_length(1000);
    buffer
}

// #[wasm_bindgen]
// pub fn print_wasm_memory_usage(){
//     info!("WASM memory usage : {:?}",wasm.memory);
// }
// #[wasm_bindgen]
// pub async fn set_initial_private_key(key: JsString) {
//     let mut key = PRIVATE_KEY.lock().await;
//     key = key.into();
// }

//
// #[wasm_bindgen]
// pub async fn load_wallet() {
//     info!("loading wallet...");
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.context.wallet.write().await;
//     wallet
//         .load(&mut Storage::new(Box::new(WasmIoHandler {})))
//         .await;
//     info!("loaded public key = {:?}", hex::encode(wallet.public_key));
// }
//
// #[wasm_bindgen]
// pub async fn save_wallet() {
//     info!("saving wallet...");
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.context.wallet.write().await;
//     wallet
//         .save(&mut Storage::new(Box::new(WasmIoHandler {})))
//         .await;
// }
//
// #[wasm_bindgen]
// pub async fn reset_wallet() {
//     info!("resetting wallet...");
//     let saito = SAITO.lock().await;
//     let mut wallet = saito.context.wallet.write().await;
//     wallet
//         .reset(&mut Storage::new(Box::new(WasmIoHandler {})))
//         .await;
// }

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

pub fn string_to_key<T: TryFrom<Vec<u8>>>(key: JsString) -> Result<T, std::io::Error>
where
    <T as TryFrom<Vec<u8>>>::Error: std::fmt::Debug,
{
    let str = key.as_string();
    if str.is_none() {
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let str = str.unwrap();
    let key = hex::decode(str);
    if key.is_err() {
        error!("{:?}", key.err().unwrap());
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let key = key.unwrap();
    let key = key.try_into();
    if key.is_err() {
        // error!("{:?}", key.err().unwrap());
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let key = key.unwrap();
    Ok(key)
}

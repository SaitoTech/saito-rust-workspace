use std::sync::Arc;
use std::time::Duration;

use js_sys::{Array, BigInt, Uint8Array};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, RwLock};
use wasm_bindgen::prelude::*;

use saito_core::common::defs::{Currency, SaitoHash, SaitoPublicKey, SaitoSignature};
use saito_core::common::process_event::ProcessEvent;
use saito_core::core::blockchain_controller::{BlockchainController, BlockchainEvent};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::Configuration;
use saito_core::core::data::context::Context;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::miner::Miner;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mempool_controller::{MempoolController, MempoolEvent};
use saito_core::core::miner_controller::{MinerController, MinerEvent};

use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_task_runner::WasmTaskRunner;
use crate::wasm_time_keeper::WasmTimeKeeper;

#[wasm_bindgen]
pub struct SaitoWasm {
    blockchain_controller: BlockchainController,
    mempool_controller: MempoolController,
    miner_controller: MinerController,
    receiver_in_blockchain: Receiver<BlockchainEvent>,
    receiver_in_mempool: Receiver<MempoolEvent>,
    receiver_in_miner: Receiver<MinerEvent>,
    context: Context,
}

// #[derive(Serialize, Deserialize)]
#[wasm_bindgen]
pub struct WasmSlip {
    pub(crate) public_key: SaitoPublicKey,
    pub(crate) uuid: SaitoHash,
    pub(crate) amount: Currency,
}

#[wasm_bindgen]
impl WasmSlip {
    pub fn new() -> WasmSlip {
        WasmSlip {
            public_key: [0; 33],
            uuid: [0; 32],
            amount: 0,
        }
    }
}

#[wasm_bindgen]
pub struct WasmTransaction {
    pub(crate) from: Vec<WasmSlip>,
    pub(crate) to: Vec<WasmSlip>,
    pub(crate) fees_total: Currency,
    pub timestamp: u64,
    pub(crate) signature: SaitoSignature,
}

#[wasm_bindgen]
impl WasmTransaction {
    pub fn new() -> WasmTransaction {
        WasmTransaction {
            from: vec![],
            to: vec![],
            fees_total: 0,
            timestamp: 0,
            signature: [0; 64],
        }
    }
    pub fn add_to_slip(&mut self, slip: WasmSlip) {}
    pub fn add_from_slip(&mut self, slip: WasmSlip) {}
    pub fn get_to_slips(&self) -> Array {
        todo!()
    }
    pub fn get_from_slips(&self) -> Array {
        todo!()
    }
    fn from_transaction(transaction: Transaction) -> WasmTransaction {
        todo!()
    }
}

lazy_static! {
    static ref SAITO: Mutex<SaitoWasm> = Mutex::new(SaitoWasm::new());
}

#[wasm_bindgen]
impl SaitoWasm {
    pub fn new() -> SaitoWasm {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let configuration = Arc::new(RwLock::new(Configuration::new()));

        let context = Context {
            blockchain: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
            mempool: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
            wallet: wallet.clone(),
            peers: Arc::new(RwLock::new(PeerCollection::new())),
            miner: Arc::new(RwLock::new(Miner::new())),
            configuration: configuration.clone(),
        };

        let (sender_to_mempool, receiver_in_mempool) = tokio::sync::mpsc::channel(100);
        let (sender_to_blockchain, receiver_in_blockchain) = tokio::sync::mpsc::channel(100);
        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(100);
        SaitoWasm {
            blockchain_controller: BlockchainController {
                blockchain: context.blockchain.clone(),
                sender_to_mempool: sender_to_mempool.clone(),
                sender_to_miner: sender_to_miner.clone(),
                peers: context.peers.clone(),
                static_peers: vec![],
                configs: context.configuration.clone(),
                io_handler: Box::new(WasmIoHandler {}),
                time_keeper: Box::new(WasmTimeKeeper {}),
            },
            mempool_controller: MempoolController {
                mempool: context.mempool.clone(),
                blockchain: context.blockchain.clone(),
                wallet: context.wallet.clone(),
                sender_to_blockchain: sender_to_blockchain.clone(),
                sender_to_miner: sender_to_miner.clone(),
                // sender_global: (),
                block_producing_timer: 0,
                tx_producing_timer: 0,
                time_keeper: Box::new(WasmTimeKeeper {}),
            },
            miner_controller: MinerController {
                miner: context.miner.clone(),
                sender_to_blockchain: sender_to_blockchain.clone(),
                sender_to_mempool: sender_to_mempool.clone(),
                time_keeper: Box::new(WasmTimeKeeper {}),
            },
            receiver_in_blockchain,
            receiver_in_mempool,
            receiver_in_miner,
            context,
        }
    }

    pub async fn create_transaction() -> Result<WasmTransaction, JsValue> {
        let saito = SAITO.lock().await;
        let wallet = saito.context.wallet.write().await;
        let transaction = wallet.create_transaction_with_default_fees().await;
        let wasm_transaction = WasmTransaction::from_transaction(transaction);
        return Ok(wasm_transaction);
    }

    pub async fn send_transaction(transaction: WasmTransaction) -> Result<JsValue, JsValue> {
        // todo : convert transaction

        let saito = SAITO.lock().await;
        // saito.blockchain_controller.
        Ok(JsValue::from("test"))
    }

    pub fn get_latest_block_hash() -> Result<JsValue, JsValue> {
        Ok(JsValue::from("latestblockhash"))
    }

    pub fn get_public_key() -> Result<JsValue, JsValue> {
        Ok(JsValue::from("publickey"))
    }

    pub async fn process_timer_event(duration: u64) {
        let mut saito = SAITO.lock().await;

        let duration = Duration::new(0, 1_000_000 * duration as u32);

        // blockchain controller
        let result = saito.receiver_in_blockchain.try_recv();
        if result.is_ok() {
            let event = result.unwrap();
            let result = saito.blockchain_controller.process_event(event).await;
        }

        saito
            .blockchain_controller
            .process_timer_event(duration.clone())
            .await;
        // mempool controller
        let result = saito.receiver_in_mempool.try_recv();
        if result.is_ok() {
            let event = result.unwrap();
            let result = saito.mempool_controller.process_event(event).await;
        }
        saito
            .mempool_controller
            .process_timer_event(duration.clone())
            .await;

        // miner controller
        let result = saito.receiver_in_miner.try_recv();
        if result.is_ok() {
            let event = result.unwrap();
            let result = saito.miner_controller.process_event(event).await;
        }
        saito.miner_controller.process_timer_event(duration.clone());
    }
}

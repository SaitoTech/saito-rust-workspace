use js_sys::JsString;
use log::info;
use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::RwLock;
use wasm_bindgen::JsValue;

use saito_core::common::defs::Currency;
use saito_core::core::data::storage::Storage;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_transaction::WasmTransaction;
use saito_core::core::data::wallet::Wallet;

#[wasm_bindgen]
#[derive(Clone)]
pub struct WasmWallet {
    pub(crate) wallet: Arc<RwLock<Wallet>>,
}

#[wasm_bindgen]
impl WasmWallet {
    pub async fn save(&self) {
        Wallet::save(self.wallet.clone(), Box::new(WasmIoHandler {})).await;
    }
    pub async fn reset(&mut self) {
        self.wallet
            .write()
            .await
            .reset(&mut Storage::new(Box::new(WasmIoHandler {})))
            .await;
    }
    pub async fn load(&mut self) {
        Wallet::load(self.wallet.clone(), Box::new(WasmIoHandler {})).await;
        // info!("loaded public key = {:?}", hex::encode(wallet.public_key));
    }

    pub async fn get_public_key(&self) -> JsString {
        let wallet = self.wallet.read().await;
        JsString::from(hex::encode(wallet.public_key))
    }
    pub async fn get_private_key(&self) -> JsString {
        let wallet = self.wallet.read().await;
        JsString::from(hex::encode(wallet.private_key))
    }
    pub async fn get_balance(&self) -> Currency {
        let wallet = self.wallet.read().await;
        wallet.get_available_balance()
    }
    pub async fn get_pending_txs(&self) -> js_sys::Array {
        let wallet = self.wallet.read().await;
        let array = js_sys::Array::new_with_length(wallet.pending_txs.len() as u32);
        for (i, tx) in wallet.pending_txs.values().enumerate() {
            let t = WasmTransaction::from_transaction(tx.clone());
            array.set(i as u32, JsValue::from(t));
        }
        array
    }
}

impl WasmWallet {
    pub fn new_from(wallet: Arc<RwLock<Wallet>>) -> WasmWallet {
        WasmWallet { wallet }
    }
}

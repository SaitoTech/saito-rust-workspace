use js_sys::JsString;
use log::error;

use std::sync::Arc;
use tokio::sync::RwLock;
use wasm_bindgen::JsValue;

use saito_core::common::defs::{Currency, SaitoPrivateKey, SaitoPublicKey};
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
        let mut wallet = self.wallet.write().await;
        Wallet::save(&mut wallet, Box::new(WasmIoHandler {})).await;
    }
    pub async fn reset(&mut self) {
        self.wallet
            .write()
            .await
            .reset(&mut Storage::new(Box::new(WasmIoHandler {})))
            .await;
    }
    pub async fn load(&mut self) {
        let mut wallet = self.wallet.write().await;
        Wallet::load(&mut wallet, Box::new(WasmIoHandler {})).await;
        // info!("loaded public key = {:?}", hex::encode(wallet.public_key));
    }

    pub async fn get_public_key(&self) -> JsString {
        let wallet = self.wallet.read().await;
        JsString::from(hex::encode(wallet.public_key))
    }
    pub async fn set_public_key(&mut self, key: JsString) {
        let str: String = key.into();
        if str.len() != 66 {
            error!(
                "invalid length : {:?} for public key string. expected 66",
                str.len()
            );
            return;
        }
        let key = hex::decode(str);
        if key.is_err() {
            error!("{:?}", key.err().unwrap());
            return;
        }
        let key = key.unwrap();
        let key: SaitoPublicKey = key.try_into().unwrap();
        let mut wallet = self.wallet.write().await;
        wallet.public_key = key;
    }
    pub async fn get_private_key(&self) -> JsString {
        let wallet = self.wallet.read().await;
        JsString::from(hex::encode(wallet.private_key))
    }
    pub async fn set_private_key(&mut self, key: JsString) {
        let str: String = key.into();
        if str.len() != 64 {
            error!(
                "invalid length : {:?} for public key string. expected 64",
                str.len()
            );
            return;
        }
        let key = hex::decode(str);
        if key.is_err() {
            error!("{:?}", key.err().unwrap());
            return;
        }
        let key = key.unwrap();
        let key: SaitoPrivateKey = key.try_into().unwrap();
        let mut wallet = self.wallet.write().await;
        wallet.private_key = key;
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

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use js_sys::JsString;
use log::{error, warn};
use tokio::sync::RwLock;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::{Currency, SaitoPrivateKey, SaitoPublicKey};
use saito_core::core::data::network::Network;
use saito_core::core::data::slip::Slip;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::wallet::{Wallet, WalletSlip};

use crate::saitowasm::string_to_key;
use crate::wasm_io_handler::WasmIoHandler;
use crate::wasm_transaction::WasmTransaction;

#[wasm_bindgen]
#[derive(Clone)]
pub struct WasmWallet {
    pub(crate) wallet: Arc<RwLock<Wallet>>,
    pub(crate) network: Arc<Network>,
}

#[wasm_bindgen]
pub struct WasmWalletSlip {
    slip: WalletSlip,
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
            .reset(&mut Storage::new(Box::new(WasmIoHandler {})), None)
            .await;
    }
    pub async fn load(&mut self) {
        let mut wallet = self.wallet.write().await;
        Wallet::load(&mut wallet, Box::new(WasmIoHandler {})).await;
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
    pub async fn get_slips(&self) -> js_sys::Array {
        let wallet = self.wallet.read().await;
        let slips = &wallet.slips;

        let array = js_sys::Array::new_with_length(slips.len() as u32);

        for (index, (key, slip)) in slips.iter().enumerate() {
            array.set(
                index as u32,
                JsValue::from(WasmWalletSlip::new(slip.clone())),
            );
        }
        array
    }
    pub async fn add_slip(&mut self, slip: WasmWalletSlip) {
        let wallet_slip = slip.slip;
        let mut wallet = self.wallet.write().await;
        let slip = Slip::parse_slip_from_utxokey(&wallet_slip.utxokey);
        wallet.add_slip(
            slip.block_id,
            slip.tx_ordinal,
            &slip,
            wallet_slip.lc,
            Some(self.network.deref()),
        );
    }
}

impl WasmWallet {
    pub fn new_from(wallet: Arc<RwLock<Wallet>>, network: Network) -> WasmWallet {
        WasmWallet {
            wallet,
            network: Arc::new(network),
        }
    }
}

impl WasmWalletSlip {
    pub fn new(slip: WalletSlip) -> WasmWalletSlip {
        WasmWalletSlip { slip: slip }
    }
}

#[wasm_bindgen]
impl WasmWalletSlip {
    pub fn get_utxokey(&self) -> js_sys::JsString {
        let key: String = hex::encode(self.slip.utxokey);
        key.into()
    }
    pub fn set_utxokey(&mut self, key: js_sys::JsString) {
        if let Ok(key) = string_to_key(key) {
            self.slip.utxokey = key;
        } else {
            warn!("failed parsing utxo key");
        }
    }
    pub fn get_amount(&self) -> Currency {
        self.slip.amount
    }
    pub fn set_amount(&mut self, amount: Currency) {
        self.slip.amount = amount;
    }
    pub fn get_block_id(&self) -> u64 {
        self.slip.block_id
    }
    pub fn set_block_id(&mut self, block_id: u64) {
        self.slip.block_id = block_id;
    }
    pub fn get_tx_ordinal(&self) -> u64 {
        self.slip.tx_ordinal
    }

    pub fn set_tx_ordinal(&mut self, ordinal: u64) {
        self.slip.tx_ordinal = ordinal;
    }

    pub fn get_slip_index(&self) -> u8 {
        self.slip.slip_index
    }

    pub fn set_slip_index(&mut self, index: u8) {
        self.slip.slip_index = index;
    }
    pub fn is_spent(&self) -> bool {
        self.slip.spent
    }
    pub fn set_spent(&mut self, spent: bool) {
        self.slip.spent = spent;
    }
    pub fn is_lc(&self) -> bool {
        self.slip.lc
    }
    pub fn set_lc(&mut self, lc: bool) {
        self.slip.lc = lc;
    }
    #[wasm_bindgen(constructor)]
    pub fn new_() -> WasmWalletSlip {
        WasmWalletSlip {
            slip: WalletSlip::new(),
        }
    }
}

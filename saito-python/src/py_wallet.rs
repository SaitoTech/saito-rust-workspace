use std::ops::Deref;
use std::sync::Arc;

use log::{error, warn};
use pyo3::pyclass;
use tokio::sync::RwLock;

use saito_core::core::consensus::slip::Slip;
use saito_core::core::consensus::wallet::{Wallet, WalletSlip};
use saito_core::core::defs::{Currency, PrintForLog, SaitoPrivateKey, SaitoPublicKey};
use saito_core::core::io::network::Network;
use saito_core::core::io::storage::Storage;

use crate::py_io_handler::PyIoHandler;
use crate::py_transaction::PyTransaction;
use crate::saitopython::string_to_hex;

#[pyclass]
#[derive(Clone)]
pub struct PyWallet {
    pub(crate) wallet: Arc<RwLock<Wallet>>,
    pub(crate) network: Arc<Network>,
}

#[pyclass]
pub struct PyWalletSlip {
    slip: WalletSlip,
}

impl PyWallet {
    pub async fn save(&self) {
        let mut wallet = self.wallet.write().await;
        Wallet::save(&mut wallet, &(PyIoHandler {})).await;
    }
    pub async fn reset(&mut self) {
        self.wallet
            .write()
            .await
            .reset(&mut Storage::new(Box::new(PyIoHandler {})), None)
            .await;
    }

    pub async fn load(&mut self) {
        let mut wallet = self.wallet.write().await;
        Wallet::load(&mut wallet, &(PyIoHandler {})).await;
    }

    pub async fn get_public_key(&self) -> String {
        let wallet = self.wallet.read().await;
        String::from(wallet.public_key.to_base58())
    }
    pub async fn set_public_key(&mut self, key: String) {
        let str: String = key.into();
        // if str.len() != 66 {
        //     error!(
        //         "invalid length : {:?} for public key string. expected 66",
        //         str.len()
        //     );
        //     return;
        // }
        let key = SaitoPublicKey::from_base58(str.as_str());
        if key.is_err() {
            error!("{:?}", key.err().unwrap());
            return;
        }
        let key = key.unwrap();
        // let key: SaitoPublicKey = key.try_into().unwrap();
        let mut wallet = self.wallet.write().await;
        wallet.public_key = key;
    }
    pub async fn get_private_key(&self) -> String {
        let wallet = self.wallet.read().await;
        String::from(wallet.private_key.to_hex())
    }
    pub async fn set_private_key(&mut self, key: String) {
        let str: String = key.into();
        if str.len() != 64 {
            error!(
                "invalid length : {:?} for public key string. expected 64",
                str.len()
            );
            return;
        }
        let key = ::hex::decode(str);
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

    pub async fn add_slip(&mut self, slip: PyWalletSlip) {
        let wallet_slip = slip.slip;
        let mut wallet = self.wallet.write().await;
        let slip = Slip::parse_slip_from_utxokey(&wallet_slip.utxokey).unwrap();
        wallet.add_slip(
            slip.block_id,
            slip.tx_ordinal,
            &slip,
            wallet_slip.lc,
            Some(self.network.deref()),
        );
    }

    pub async fn add_to_pending(&mut self, tx: &PyTransaction) {
        let mut wallet = self.wallet.write().await;
        let mut tx = tx.clone().tx;
        tx.generate(&wallet.public_key, 0, 0);
        wallet.add_to_pending(tx);
    }

    // pub async fn set_key_list(&self, key_list: js_sys::Array) {
    //     let key_list: Vec<SaitoPublicKey> = string_array_to_base58_keys(key_list);
    //
    //     let mut saito = SAITO.lock().await;
    //     saito
    //         .as_mut()
    //         .unwrap()
    //         .routing_thread
    //         .set_my_key_list(key_list)
    //         .await;
    // }
}

impl PyWallet {
    pub fn new_from(wallet: Arc<RwLock<Wallet>>, network: Network) -> PyWallet {
        PyWallet {
            wallet,
            network: Arc::new(network),
        }
    }
}

impl PyWalletSlip {
    pub fn new(slip: WalletSlip) -> PyWalletSlip {
        PyWalletSlip { slip }
    }
}

impl PyWalletSlip {
    pub fn get_utxokey(&self) -> String {
        let key: String = self.slip.utxokey.to_hex();
        key.into()
    }
    pub fn set_utxokey(&mut self, key: String) {
        if let Ok(key) = string_to_hex(key) {
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
    pub fn new_() -> PyWalletSlip {
        PyWalletSlip {
            slip: WalletSlip::new(),
        }
    }
}

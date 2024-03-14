use ahash::{AHashMap, AHashSet};
use log::{info, trace, warn};

use crate::core::consensus::block::Block;
use crate::core::consensus::golden_ticket::GoldenTicket;
use crate::core::consensus::slip::Slip;
use crate::core::consensus::transaction::{Transaction, TransactionType};
use crate::core::defs::{
    Currency, PrintForLog, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature,
    SaitoUTXOSetKey,
};
use crate::core::io::interface_io::{InterfaceEvent, InterfaceIO};
use crate::core::io::network::Network;
use crate::core::io::storage::Storage;
use crate::core::process::version::Version;
use crate::core::util::balance_snapshot::BalanceSnapshot;
use crate::core::util::crypto::{generate_keys, hash, sign};

pub const WALLET_SIZE: usize = 65;

/// The `WalletSlip` stores the essential information needed to track which
/// slips are spendable and managing them as they move onto and off of the
/// longest-chain.
///
/// Please note that the wallet in this Saito Rust client is intended primarily
/// to hold the public/private_key and that slip-spending and tracking code is
/// not coded in a way intended to be robust against chain-reorganizations but
/// rather for testing of basic functions like transaction creation. Slips that
/// are spent on one fork are not recaptured on chains, for instance, and once
/// a slip is spent it is marked as spent.
///
#[derive(Clone, Debug, PartialEq)]
pub struct WalletSlip {
    pub utxokey: SaitoUTXOSetKey,
    pub amount: Currency,
    pub block_id: u64,
    pub tx_ordinal: u64,
    pub lc: bool,
    pub slip_index: u8,
    pub spent: bool,
}

/// The `Wallet` manages the public and private keypair of the node and holds the
/// slips that are used to form transactions on the network.
#[derive(Clone, Debug, PartialEq)]
pub struct Wallet {
    pub public_key: SaitoPublicKey,
    pub private_key: SaitoPrivateKey,
    pub slips: AHashMap<SaitoUTXOSetKey, WalletSlip>,
    unspent_slips: AHashSet<SaitoUTXOSetKey>,
    pub filename: String,
    pub filepass: String,
    available_balance: Currency,
    pub pending_txs: AHashMap<SaitoHash, Transaction>,
    // TODO : this version should be removed. only added as a temporary hack to allow SLR app version to be easily upgraded in browsers
    pub version: Version,
    pub key_list: Vec<SaitoPublicKey>,
}

impl Wallet {
    pub fn new(private_key: SaitoPrivateKey, public_key: SaitoPublicKey) -> Wallet {
        info!("generating new wallet...");
        // let (public_key, private_key) = generate_keys();

        Wallet {
            public_key,
            private_key,
            slips: AHashMap::new(),
            unspent_slips: AHashSet::new(),
            filename: "default".to_string(),
            filepass: "password".to_string(),
            available_balance: 0,
            pending_txs: Default::default(),
            version: Default::default(),
            key_list: vec![],
        }
    }

    pub async fn load(wallet: &mut Wallet, io: &(dyn InterfaceIO + Send + Sync)) {
        info!("loading wallet...");
        let result = io.load_wallet(wallet).await;
        if result.is_err() {
            warn!("loading wallet failed. saving new wallet");
            // TODO : check error code
            io.save_wallet(wallet).await.unwrap();
        } else {
            info!("wallet loaded");
            io.send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }
    pub async fn save(wallet: &mut Wallet, io: &(dyn InterfaceIO + Send + Sync)) {
        trace!("saving wallet");
        io.save_wallet(wallet).await.unwrap();
        trace!("wallet saved");
    }

    pub async fn reset(&mut self, _storage: &mut Storage, network: Option<&Network>) {
        info!("resetting wallet");
        let keys = generate_keys();
        self.public_key = keys.0;
        self.private_key = keys.1;
        self.pending_txs.clear();
        self.available_balance = 0;
        self.slips.clear();
        self.unspent_slips.clear();
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }

    /// [private_key - 32 bytes]
    /// [public_key - 33 bytes]
    pub fn serialize_for_disk(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];

        vbytes.extend(&self.private_key);
        vbytes.extend(&self.public_key);

        // TODO : should we write key list here for rust nodes ?

        vbytes
    }

    /// [private_key - 32 bytes]
    /// [public_key - 33 bytes]
    pub fn deserialize_from_disk(&mut self, bytes: &Vec<u8>) {
        self.private_key = bytes[0..32].try_into().unwrap();
        self.public_key = bytes[32..65].try_into().unwrap();
    }

    pub fn on_chain_reorganization(&mut self, block: &Block, lc: bool, network: Option<&Network>) {
        if lc {
            for (index, tx) in block.transactions.iter().enumerate() {
                for input in tx.from.iter() {
                    if input.amount > 0 && input.public_key == self.public_key {
                        self.delete_slip(input, None);
                    }
                }
                for output in tx.to.iter() {
                    if output.amount > 0 && output.public_key == self.public_key {
                        self.add_slip(block.id, index as u64, output, true, None);
                    }
                }
            }
        } else {
            for (index, tx) in block.transactions.iter().enumerate() {
                for input in tx.from.iter() {
                    if input.amount > 0 && input.public_key == self.public_key {
                        self.add_slip(block.id, index as u64, input, true, None);
                    }
                }
                for output in tx.to.iter() {
                    if output.amount > 0 && output.public_key == self.public_key {
                        self.delete_slip(output, None);
                    }
                }
            }
        }
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }

    // removes all slips in block when pruned / deleted
    pub fn delete_block(&mut self, block: &Block, network: Option<&Network>) {
        for tx in block.transactions.iter() {
            for input in tx.from.iter() {
                self.delete_slip(input, None);
            }
            for output in tx.to.iter() {
                if output.amount > 0 {
                    self.delete_slip(output, None);
                }
            }
        }
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }

    pub fn add_slip(
        &mut self,
        block_id: u64,
        tx_index: u64,
        slip: &Slip,
        lc: bool,
        network: Option<&Network>,
    ) {
        if self.slips.contains_key(&slip.get_utxoset_key()) {
            return;
        }
        let mut wallet_slip = WalletSlip::new();
        assert_ne!(block_id, 0);
        wallet_slip.utxokey = slip.get_utxoset_key();
        wallet_slip.amount = slip.amount;
        wallet_slip.slip_index = slip.slip_index;
        wallet_slip.block_id = block_id;
        wallet_slip.tx_ordinal = tx_index;
        wallet_slip.lc = lc;
        self.unspent_slips.insert(wallet_slip.utxokey);
        self.available_balance += slip.amount;
        trace!(
            "adding slip : {:?} with value : {:?} to wallet",
            wallet_slip.utxokey.to_hex(),
            wallet_slip.amount
        );
        self.slips.insert(wallet_slip.utxokey, wallet_slip);
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }

    pub fn delete_slip(&mut self, slip: &Slip, network: Option<&Network>) {
        trace!(
            "deleting slip : {:?} with value : {:?} from wallet",
            slip.utxoset_key.to_hex(),
            slip.amount
        );
        let result = self.slips.remove(&slip.utxoset_key);
        let in_unspent_list = self.unspent_slips.remove(&slip.utxoset_key);
        if let Some(removed_slip) = result {
            if in_unspent_list {
                self.available_balance -= removed_slip.amount;
            }
        }
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }

    pub fn get_available_balance(&self) -> Currency {
        self.available_balance
    }

    pub fn get_unspent_slip_count(&self) -> u64 {
        self.unspent_slips.len() as u64
    }

    // the nolan_requested is omitted from the slips created - only the change
    // address is provided as an output. so make sure that any function calling
    // this manually creates the output for its desired payment
    pub fn generate_slips(
        &mut self,
        nolan_requested: Currency,
        network: Option<&Network>,
    ) -> (Vec<Slip>, Vec<Slip>) {
        let mut inputs: Vec<Slip> = Vec::new();
        let mut outputs: Vec<Slip> = Vec::new();
        let mut nolan_in: Currency = 0;
        let mut nolan_out: Currency = 0;
        let my_public_key = self.public_key;

        // grab inputs
        let mut keys_to_remove = Vec::new();
        for key in &self.unspent_slips {
            if nolan_in >= nolan_requested {
                break;
            }
            let slip = self.slips.get_mut(key).expect("slip should be here");
            nolan_in += slip.amount;

            let mut input = Slip::default();
            input.public_key = my_public_key;
            input.amount = slip.amount;
            input.block_id = slip.block_id;
            input.tx_ordinal = slip.tx_ordinal;
            input.slip_index = slip.slip_index;
            inputs.push(input);

            slip.spent = true;
            self.available_balance -= slip.amount;

            trace!(
                "marking slip : {:?} with value : {:?} as spent",
                slip.utxokey.to_hex(),
                slip.amount
            );
            keys_to_remove.push(slip.utxokey);
        }

        for key in keys_to_remove {
            self.unspent_slips.remove(&key);
        }

        // create outputs
        if nolan_in > nolan_requested {
            nolan_out = nolan_in - nolan_requested;
        }

        // add change address
        let mut output = Slip::default();
        output.public_key = my_public_key;
        output.amount = nolan_out;
        outputs.push(output);

        // ensure not empty
        if inputs.is_empty() {
            let mut input = Slip::default();
            input.public_key = my_public_key;
            input.amount = 0;
            input.block_id = 0;
            input.tx_ordinal = 0;
            inputs.push(input);
        }
        if outputs.is_empty() {
            let mut output = Slip::default();
            output.public_key = my_public_key;
            output.amount = 0;
            output.block_id = 0;
            output.tx_ordinal = 0;
            outputs.push(output);
        }
        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }

        (inputs, outputs)
    }

    pub fn sign(&self, message_bytes: &[u8]) -> SaitoSignature {
        sign(message_bytes, &self.private_key)
    }

    pub async fn create_golden_ticket_transaction(
        golden_ticket: GoldenTicket,
        public_key: &SaitoPublicKey,
        private_key: &SaitoPrivateKey,
    ) -> Transaction {
        let mut transaction = Transaction::default();

        // for now we'll use bincode to de/serialize
        transaction.transaction_type = TransactionType::GoldenTicket;
        transaction.data = golden_ticket.serialize_for_net();

        let mut input1 = Slip::default();
        input1.public_key = public_key.clone();
        input1.amount = 0;
        input1.block_id = 0;
        input1.tx_ordinal = 0;

        let mut output1 = Slip::default();
        output1.public_key = public_key.clone();
        output1.amount = 0;
        output1.block_id = 0;
        output1.tx_ordinal = 0;

        transaction.add_from_slip(input1);
        transaction.add_to_slip(output1);

        let hash_for_signature: SaitoHash = hash(&transaction.serialize_for_signature());
        transaction.hash_for_signature = Some(hash_for_signature);

        transaction.sign(private_key);

        transaction
    }
    pub fn add_to_pending(&mut self, tx: Transaction) {
        assert_eq!(tx.from.get(0).unwrap().public_key, self.public_key);
        assert_ne!(tx.transaction_type, TransactionType::GoldenTicket);
        assert!(tx.hash_for_signature.is_some());
        self.pending_txs.insert(tx.hash_for_signature.unwrap(), tx);
    }
    pub fn update_from_balance_snapshot(
        &mut self,
        snapshot: BalanceSnapshot,
        network: Option<&Network>,
    ) {
        // need to reset balance and slips to avoid failing integrity from forks
        self.unspent_slips.clear();
        self.slips.clear();
        self.available_balance = 0;

        snapshot.slips.iter().for_each(|slip| {
            assert_ne!(slip.utxoset_key, [0; 58]);
            let wallet_slip = WalletSlip {
                utxokey: slip.utxoset_key,
                amount: slip.amount,
                block_id: slip.block_id,
                tx_ordinal: slip.tx_ordinal,
                lc: true,
                slip_index: slip.slip_index,
                spent: false,
            };
            let result = self.slips.insert(slip.utxoset_key, wallet_slip);
            if result.is_none() {
                self.unspent_slips.insert(slip.utxoset_key);
                self.available_balance += slip.amount;
                info!("slip key : {:?} with value : {:?} added to wallet from snapshot for address : {:?}",
                    slip.utxoset_key.to_hex(),
                    slip.amount,
                    slip.public_key.to_base58());
            } else {
                info!(
                    "slip with utxo key : {:?} was already available",
                    slip.utxoset_key.to_hex()
                );
            }
        });

        if let Some(network) = network {
            network
                .io_interface
                .send_interface_event(InterfaceEvent::WalletUpdate());
        }
    }
    pub fn set_key_list(&mut self, key_list: Vec<SaitoPublicKey>) {
        self.key_list = key_list;
    }
}

impl WalletSlip {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        WalletSlip {
            utxokey: [0; 58],
            amount: 0,
            block_id: 0,
            tx_ordinal: 0,
            lc: true,
            slip_index: 0,
            spent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::consensus::wallet::Wallet;
    use crate::core::defs::SaitoPublicKey;
    use crate::core::io::storage::Storage;
    use crate::core::util::crypto::generate_keys;
    use crate::core::util::test::test_manager::test::TestManager;

    use super::*;

    #[test]
    fn wallet_new_test() {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);
        assert_ne!(wallet.public_key, [0; 33]);
        assert_ne!(wallet.private_key, [0; 32]);
        assert_eq!(wallet.serialize_for_disk().len(), WALLET_SIZE);
    }

    // tests value transfer to other addresses and verifies the resulting utxo hashmap
    #[tokio::test]
    #[serial_test::serial]
    async fn wallet_transfer_to_address_test() {
        let mut t = TestManager::default();
        t.initialize(100, 100000).await;

        let mut last_param = 120000;

        let public_keys = [
            "s8oFPjBX97NC2vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR",
            "s9adoFPjBX972vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR",
            "s223oFPjBX97NC2bmE5Kd2oHWUShuSTUuZwSB1U4wsPR",
        ];

        for &public_key_string in &public_keys {
            let public_key = Storage::decode_str(public_key_string).unwrap();
            let mut to_public_key: SaitoPublicKey = [0u8; 33];
            to_public_key.copy_from_slice(&public_key);
            t.transfer_value_to_public_key(to_public_key, 500, last_param)
                .await
                .unwrap();
            let balance_map = t.balance_map().await;
            let their_balance = *balance_map.get(&to_public_key).unwrap();
            assert_eq!(500, their_balance);

            last_param += 120000;
        }

        let my_balance = t.get_balance().await;

        let expected_balance = 10000000 - 500 * public_keys.len() as u64; // 500 is the amount transferred each time
        assert_eq!(expected_balance, my_balance);
    }

    // Test if transfer is possible even with issufficient funds
    #[tokio::test]
    #[serial_test::serial]
    async fn transfer_with_insufficient_funds_failure_test() {
        let mut t = TestManager::default();
        t.initialize(100, 100000).await;
        let public_key_string = "s8oFPjBX97NC2vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR";
        let public_key = Storage::decode_str(public_key_string).unwrap();
        let mut to_public_key: SaitoPublicKey = [0u8; 33];
        to_public_key.copy_from_slice(&public_key);

        // Try transferring more than what the wallet contains
        let result = t
            .transfer_value_to_public_key(to_public_key, 1000000000, 120000)
            .await;
        assert!(result.is_err() || !result.is_ok());
    }

    // tests transfer of exact amount
    #[tokio::test]
    #[serial_test::serial]
    async fn test_transfer_with_exact_funds() {
        // pretty_env_logger::init();
        let mut t = TestManager::default();
        t.initialize(1, 500).await;

        let public_key_string = "s8oFPjBX97NC2vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR";
        let public_key = Storage::decode_str(public_key_string).unwrap();
        let mut to_public_key: SaitoPublicKey = [0u8; 33];
        to_public_key.copy_from_slice(&public_key);

        t.transfer_value_to_public_key(to_public_key, 500, 120000)
            .await
            .unwrap();

        let balance_map = t.balance_map().await;

        let their_balance = *balance_map.get(&to_public_key).unwrap();
        assert_eq!(500, their_balance);
        let my_balance = t.get_balance().await;
        assert_eq!(0, my_balance);
    }

    #[test]
    fn wallet_serialize_and_deserialize_test() {
        let keys = generate_keys();
        let wallet1 = Wallet::new(keys.1, keys.0);
        let keys = generate_keys();
        let mut wallet2 = Wallet::new(keys.1, keys.0);
        let serialized = wallet1.serialize_for_disk();
        wallet2.deserialize_from_disk(&serialized);
        assert_eq!(wallet1, wallet2);
    }

    // #[tokio::test]
    // #[serial_test::serial]
    // async fn save_and_restore_wallet_test() {
    //     info!("current dir = {:?}", std::env::current_dir().unwrap());
    //
    //     let _t = TestManager::new();
    //
    //     let keys = generate_keys();
    //     let mut wallet = Wallet::new(keys.1, keys.0);
    //     let public_key1 = wallet.public_key.clone();
    //     let private_key1 = wallet.private_key.clone();
    //
    //     let mut storage = Storage {
    //         io_interface: Box::new(TestIOHandler::new()),
    //     };
    //     wallet.save(&mut storage).await;
    //
    //     let keys = generate_keys();
    //     wallet = Wallet::new(keys.1, keys.0);
    //
    //     assert_ne!(wallet.public_key, public_key1);
    //     assert_ne!(wallet.private_key, private_key1);
    //
    //     wallet.load(&mut storage).await;
    //
    //     assert_eq!(wallet.public_key, public_key1);
    //     assert_eq!(wallet.private_key, private_key1);
    // }
}

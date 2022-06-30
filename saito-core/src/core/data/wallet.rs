use crate::common::defs::{
    SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey,
};
use crate::core::data::block::Block;
use crate::core::data::crypto::{
    decrypt_with_password, encrypt_with_password, generate_keys, hash, sign,
};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::slip::Slip;
use crate::core::data::storage::Storage;
use crate::core::data::transaction::{Transaction, TransactionType};

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
    pub uuid: SaitoHash,
    pub utxokey: SaitoUTXOSetKey,
    pub amount: u64,
    pub block_id: u64,
    pub block_hash: SaitoHash,
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
    slips: Vec<WalletSlip>,
    pub filename: String,
    pub filepass: String,
}

impl Wallet {
    pub fn new() -> Wallet {
        let (public_key, private_key) = generate_keys();
        Wallet {
            public_key,
            private_key,
            slips: vec![],
            filename: "default".to_string(),
            filepass: "password".to_string(),
        }
    }

    pub async fn load(&mut self, storage: &mut Storage) {
        let mut filename = String::from("data/wallets/");
        filename.push_str(&self.filename);

        if storage.file_exists(&filename).await {
            let password = self.filepass.clone();
            let encoded = storage.read(&filename).await.unwrap();
            let decrypted_encoded = decrypt_with_password(encoded, &password);
            self.deserialize_from_disk(&decrypted_encoded);
        } else {
            //
            // new wallet, save to disk
            //
            self.save(storage).await;
        }
    }

    pub async fn load_wallet(
        &mut self,
        wallet_path: &str,
        password: Option<&str>,
        storage: &mut Storage,
    ) {
        self.filename = wallet_path.to_string();
        self.filepass = password.unwrap().to_string();
        self.load(storage).await;
    }

    pub async fn save(&mut self, storage: &mut Storage) {
        let mut filename = String::from("data/wallets/");
        filename.push_str(&self.filename);

        let password = self.filepass.clone();
        let byte_array: Vec<u8> = self.serialize_for_disk();
        let encrypted_wallet = encrypt_with_password((&byte_array[..]).to_vec(), &password);

        storage.write(encrypted_wallet, &filename).await;
    }

    /// [private_key - 32 bytes]
    /// [public_key - 33 bytes]
    pub fn serialize_for_disk(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];

        vbytes.extend(&self.private_key);
        vbytes.extend(&self.public_key);

        vbytes
    }

    /// [private_key - 32 bytes
    /// [public_key - 33 bytes]
    pub fn deserialize_from_disk(&mut self, bytes: &Vec<u8>) {
        self.private_key = bytes[0..32].try_into().unwrap();
        self.public_key = bytes[32..65].try_into().unwrap();
    }

    pub fn on_chain_reorganization(&mut self, block: &Block, lc: bool) {
        if lc {
            for tx in block.transactions.iter() {
                for input in tx.inputs.iter() {
                    if input.amount > 0 && input.public_key == self.public_key {
                        self.delete_slip(input);
                    }
                }
                for output in tx.outputs.iter() {
                    if output.amount > 0 && output.public_key == self.public_key {
                        self.add_slip(block, tx, output, true);
                    }
                }
            }
        } else {
            for tx in block.transactions.iter() {
                for input in tx.inputs.iter() {
                    if input.amount > 0 && input.public_key == self.public_key {
                        self.add_slip(block, tx, input, true);
                    }
                }
                for output in tx.outputs.iter() {
                    if output.amount > 0 && output.public_key == self.public_key {
                        self.delete_slip(output);
                    }
                }
            }
        }
    }

    //
    // removes all slips in block when pruned / deleted
    //
    pub fn delete_block(&mut self, block: &Block) {
        for tx in block.transactions.iter() {
            for input in tx.inputs.iter() {
                self.delete_slip(input);
            }
            for output in tx.outputs.iter() {
                if output.amount > 0 {
                    self.delete_slip(output);
                }
            }
        }
    }

    pub fn add_slip(&mut self, block: &Block, transaction: &Transaction, slip: &Slip, lc: bool) {
        let mut wallet_slip = WalletSlip::new();

        wallet_slip.uuid = transaction.hash_for_signature.unwrap();
        wallet_slip.utxokey = slip.get_utxoset_key();
        wallet_slip.amount = slip.amount;
        wallet_slip.slip_index = slip.slip_index;
        wallet_slip.block_id = block.id;
        wallet_slip.block_hash = block.hash;
        wallet_slip.lc = lc;
        self.slips.push(wallet_slip);
    }

    pub fn delete_slip(&mut self, slip: &Slip) {
        self.slips
            .retain(|x| x.uuid != slip.uuid || x.slip_index != slip.slip_index);
    }

    pub fn get_available_balance(&self) -> u64 {
        let mut available_balance: u64 = 0;
        for slip in &self.slips {
            if !slip.spent {
                available_balance += slip.amount;
            }
        }
        available_balance
    }

    // the nolan_requested is omitted from the slips created - only the change
    // address is provided as an output. so make sure that any function calling
    // this manually creates the output for its desired payment
    pub fn generate_slips(&mut self, nolan_requested: u64) -> (Vec<Slip>, Vec<Slip>) {
        let mut inputs: Vec<Slip> = vec![];
        let mut outputs: Vec<Slip> = vec![];
        let mut nolan_in: u64 = 0;
        let mut nolan_out: u64 = 0;
        let my_public_key = self.public_key;

        //
        // grab inputs
        //
        for slip in &mut self.slips {
            if !slip.spent {
                if nolan_in < nolan_requested {
                    nolan_in += slip.amount;

                    let mut input = Slip::new();
                    input.public_key = my_public_key;
                    input.amount = slip.amount;
                    input.uuid = slip.uuid;
                    input.slip_index = slip.slip_index;
                    inputs.push(input);

                    slip.spent = true;
                }
            }
        }

        //
        // create outputs
        //
        if nolan_in > nolan_requested {
            nolan_out = nolan_in - nolan_requested;
        }

        //
        // add change address
        //
        let mut output = Slip::new();
        output.public_key = my_public_key;
        output.amount = nolan_out;
        outputs.push(output);

        //
        // ensure not empty
        //
        if inputs.is_empty() {
            let mut input = Slip::new();
            input.public_key = my_public_key;
            input.amount = 0;
            input.uuid = [0; 32];
            inputs.push(input);
        }
        if outputs.is_empty() {
            let mut output = Slip::new();
            output.public_key = my_public_key;
            output.amount = 0;
            output.uuid = [0; 32];
            outputs.push(output);
        }

        (inputs, outputs)
    }

    pub fn sign(&self, message_bytes: &[u8]) -> SaitoSignature {
        sign(message_bytes, self.private_key)
    }

    pub async fn create_transaction_with_default_fees(&self) -> Transaction {
        // TODO : to be implemented
        Transaction::new()
    }
    pub async fn create_golden_ticket_transaction(
        &mut self,
        golden_ticket: GoldenTicket,
    ) -> Transaction {
        let mut transaction = Transaction::new();

        // for now we'll use bincode to de/serialize
        transaction.transaction_type = TransactionType::GoldenTicket;
        transaction.message = golden_ticket.serialize_for_net();

        let mut input1 = Slip::new();
        input1.public_key = self.public_key;
        input1.amount = 0;
        input1.uuid = [0; 32];

        let mut output1 = Slip::new();
        output1.public_key = self.public_key;
        output1.amount = 0;
        output1.uuid = [0; 32];

        transaction.add_input(input1);
        transaction.add_output(output1);

        let hash_for_signature: SaitoHash = hash(&transaction.serialize_for_signature());
        transaction.hash_for_signature = Some(hash_for_signature);

        transaction.sign(self.private_key);

        transaction
    }
}

impl WalletSlip {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        WalletSlip {
            uuid: [0; 32],
            utxokey: [0; 74],
            amount: 0,
            block_id: 0,
            block_hash: [0; 32],
            lc: true,
            slip_index: 0,
            spent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use log::info;

    use crate::common::test_io_handler::test::TestIOHandler;
    use crate::common::test_manager::test::TestManager;
    use crate::core::data::wallet::Wallet;

    use super::*;

    #[test]
    fn wallet_new_test() {
        let wallet = Wallet::new();
        assert_ne!(wallet.public_key, [0; 33]);
        assert_ne!(wallet.private_key, [0; 32]);
        assert_eq!(wallet.serialize_for_disk().len(), WALLET_SIZE);
    }

    #[test]
    fn wallet_serialize_and_deserialize_test() {
        let wallet1 = Wallet::new();
        let mut wallet2 = Wallet::new();
        let serialized = wallet1.serialize_for_disk();
        wallet2.deserialize_from_disk(&serialized);
        assert_eq!(wallet1, wallet2);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn save_and_restore_wallet_test() {
        info!("current dir = {:?}", std::env::current_dir().unwrap());

        let mut t = TestManager::new();

        let mut wallet = Wallet::new();
        let public_key1 = wallet.public_key.clone();
        let private_key1 = wallet.private_key.clone();

        let mut storage = Storage {
            io_interface: Box::new(TestIOHandler::new()),
        };
        wallet.save(&mut storage).await;

        wallet = Wallet::new();

        assert_ne!(wallet.public_key, public_key1);
        assert_ne!(wallet.private_key, private_key1);

        wallet.load(&mut storage).await;

        assert_eq!(wallet.public_key, public_key1);
        assert_eq!(wallet.private_key, private_key1);
    }
}

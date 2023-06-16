use std::borrow::BorrowMut;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ahash::AHashMap;
use log::{debug, info};
use std::error::Error;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::Write;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;

use crate::test_io_handler::TestIOHandler;
use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, Timestamp,
    UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET,
};

use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::crypto::{generate_keys, generate_random_bytes, hash, verify_signature};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::MiningEvent;
use saito_core::{lock_for_read, lock_for_write};
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};
use crate::config::TestConfiguration;

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}


pub fn create_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as Timestamp
}

//struct to manage setup of chain for analytics
pub struct ChainManager {
    pub mempool_lock: Arc<RwLock<Mempool>>,
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub latest_block_hash: SaitoHash,
    pub network: Network,
    pub storage: Storage,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub receiver_in_miner: Receiver<MiningEvent>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl ChainManager {
    pub fn new() -> Self {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);
        let _public_key = wallet.public_key.clone();
        let _private_key = wallet.private_key.clone();
        let peers = Arc::new(RwLock::new(PeerCollection::new()));
        let wallet_lock = Arc::new(RwLock::new(wallet));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let mempool_lock = Arc::new(RwLock::new(Mempool::new(wallet_lock.clone())));
        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(10);
        let configs = Arc::new(RwLock::new(TestConfiguration {}));

        Self {
            wallet_lock: wallet_lock.clone(),
            blockchain_lock,
            mempool_lock,
            latest_block_hash: [0; 32],
            network: Network::new(
                Box::new(TestIOHandler::new()),
                peers.clone(),
                wallet_lock.clone(),
                configs.clone(),
            ),
            peers: peers.clone(),
            storage: Storage::new(Box::new(TestIOHandler::new())),
            sender_to_miner: sender_to_miner.clone(),
            receiver_in_miner,
            configs,
        }
    }

    pub fn get_mempool_lock(&self) -> Arc<RwLock<Mempool>> {
        return self.mempool_lock.clone();
    }

    pub fn get_wallet_lock(&self) -> Arc<RwLock<Wallet>> {
        return self.wallet_lock.clone();
    }

    pub fn get_blockchain_lock(&self) -> Arc<RwLock<Blockchain>> {
        return self.blockchain_lock.clone();
    }

    pub async fn wait_for_mining_event(&mut self) {
        self.receiver_in_miner
            .recv()
            .await
            .expect("mining event receive failed");
    }

    //
    // add block to blockchain
    //
    pub async fn add_block(&mut self, block: Block) {
        debug!("adding block to test manager blockchain");
        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        let (mut mempool, _mempool_) = lock_for_write!(self.mempool_lock, LOCK_ORDER_MEMPOOL);

        blockchain
            .add_block(
                block,
                &mut self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
                &mut mempool,
                configs.deref(),
            )
            .await;
        debug!("block added to test manager blockchain");
    }

    //TODO
    pub fn applyTxSet(tx: Transaction) {
        //set inputs to false
        //
        tx.from.iter().for_each(|input| {
            //input.on_chain_reorganization(utxoset, input_slip_spendable)
            //let input_hex = hex::encode(input.public_key);
            //utxoset.insert(input_hex, false);
        });

        tx.to.iter().for_each(|output| {
            //output.on_chain_reorganization(utxoset, longest_chain, output_slip_spendable)
        });
    }

    pub fn applyTx(&self, tx: Transaction, utxo_balances: &mut AHashMap<String, u64>) {
        //apply
        println!("applyTx");
        tx.from.iter().for_each(|input| {
            let input_hex = hex::encode(input.public_key);
            let balance = utxo_balances.entry(input_hex).or_insert(0);
            *balance -= input.amount;
            println!("bal {}", balance);
        });

        tx.to.iter().for_each(|output| {
            let output_hex = hex::encode(output.public_key);
            let balance = utxo_balances.entry(output_hex).or_insert(0);
            *balance += output.amount;
            println!("bal {}", balance);
        });
    }

    pub async fn get_blocks_vec(&self) -> Vec<Block> {
        let mut blocks = Vec::new();

        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        for i in 1..=blockchain.get_latest_block_id() {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            //println!("WINDING ID HASH - {} {:?}", i, block_hash);
            let block = blockchain.get_block(&block_hash).unwrap().clone();
            blocks.push(block);
        }

        blocks
    }

    //a function which generates utxo balances by applying each block
    //doesnt validate and not optimized
    pub async fn get_utxobalances(&self, threshold: u64) -> AHashMap<String, u64> {
        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        type UtxoSetBalance = AHashMap<String, u64>;

        let latest_block_id = blockchain.get_latest_block_id();

        //the balance state which we aggregate to
        let mut utxo_balances: UtxoSetBalance = AHashMap::new();

        //publickey \t amount \t type (normal)
        //TODO check spendable only

        let blocks = self.get_blocks_vec().await;

        for block in blocks {
            for j in 0..block.transactions.len() {
                let mut tx = &block.transactions[j];

                //println!("tx from >> {}", tx.from.len());
                //println!("tx to >> {}", tx.to.len());
                // println!("transaction_type  >> {:?}", tx.transaction_type);

                self.applyTx(tx.clone(), &mut utxo_balances);
            }
        }

        utxo_balances
    }

    //get utxo and write to file
    pub async fn dump_utxoset(&self, threshold: u64) {
        println!("dump utxoset");
        //let mut file = File::create("data/utxoset.txt"); // Use await and ? here
        let mut file = File::create("data/utxoset.txt").unwrap();

        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        println!(
            "UTXO state height: latest_block_id {}",
            blockchain.get_latest_block_id()
        );
        writeln!(
            file,
            "UTXO state height: latest_block_id {}",
            blockchain.get_latest_block_id()
        );

        let utxo_balances = self.get_utxobalances(threshold).await;

        for (key, value) in utxo_balances {
            if (value > threshold) {
                println!("{}\t{}", key, value);
                writeln!(file, "{} {:?}", key, value);
            }
        }
    }

    pub async fn create_tx(
        &mut self,
        public_key: &SaitoPublicKey,
        private_key: &SaitoPrivateKey,
        txs_number: usize,
        txs_amount: Currency,
        txs_fee: Currency,
    ) -> AHashMap<SaitoSignature, Transaction> {
        let mut transactions: AHashMap<SaitoSignature, Transaction> = Default::default();
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;

        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        for _i in 0..txs_number {
            let mut transaction;
            {
                let (mut wallet, _wallet_) = lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);

                transaction =
                    Transaction::create(&mut wallet, public_key, txs_amount, txs_fee, false)
                        .unwrap();
            }

            transaction.sign(&private_key);
            transaction.generate(&public_key, 0, 0);
            transactions.insert(transaction.signature, transaction);
        }

        return transactions;
    }

    //
    // create block
    //
    pub async fn create_block(
        &mut self,
        parent_hash: SaitoHash,
        timestamp: Timestamp,
        txs_number: usize,
        txs_amount: Currency,
        txs_fee: Currency,
        include_valid_golden_ticket: bool,
    ) -> Block {
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;
        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let mut transactions = self
            .create_tx(&public_key, &private_key, txs_number, txs_amount, txs_fee)
            .await;

        if include_valid_golden_ticket {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let block = blockchain.get_block(&parent_hash).unwrap();
            let golden_ticket: GoldenTicket =
                Self::create_golden_ticket(self.wallet_lock.clone(), parent_hash, block.difficulty)
                    .await;
            let mut gttx: Transaction;
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

                gttx = Wallet::create_golden_ticket_transaction(
                    golden_ticket,
                    &wallet.public_key,
                    &wallet.private_key,
                )
                .await;
            }
            gttx.generate(&public_key, 0, 0);
            transactions.insert(gttx.signature, gttx);
        }

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        //
        // create block
        //
        let mut block = Block::create(
            &mut transactions,
            parent_hash,
            blockchain.borrow_mut(),
            timestamp,
            &public_key,
            &private_key,
            None,
            configs.deref(),
        )
        .await;
        block.generate();
        block.sign(&private_key);

        block
    }

    pub async fn create_golden_ticket(
        wallet: Arc<RwLock<Wallet>>,
        block_hash: SaitoHash,
        block_difficulty: u64,
    ) -> GoldenTicket {
        let public_key;
        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
        }
        let mut random_bytes = hash(&generate_random_bytes(32));

        let mut gt = GoldenTicket::create(block_hash, random_bytes, public_key);

        while !gt.validate(block_difficulty) {
            random_bytes = hash(&generate_random_bytes(32));
            gt = GoldenTicket::create(block_hash, random_bytes, public_key);
        }

        GoldenTicket::new(block_hash, random_bytes, public_key)
    }

    pub async fn initialize(&mut self, vip_transactions: u64, vip_amount: Currency) {
        let timestamp = create_timestamp();
        self.initialize_with_timestamp(vip_transactions, vip_amount, timestamp)
            .await;
    }

    //
    // initialize chain
    //
    // creates and adds the first block to the blockchain, with however many VIP
    // transactions are necessary
    pub async fn initialize_with_timestamp(
        &mut self,
        vip_transactions_num: u64,
        vip_amount: Currency,
        timestamp: Timestamp,
    ) {
        //
        // initialize timestamp
        //

        //
        // reset data dirs
        //
        tokio::fs::remove_dir_all("data/blocks").await;
        tokio::fs::create_dir_all("data/blocks").await.unwrap();
        tokio::fs::remove_dir_all("data/wallets").await;
        tokio::fs::create_dir_all("data/wallets").await.unwrap();

        //
        // create initial transactions
        //
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;
        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        //
        // create first block
        //
        let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

        //
        // generate UTXO-carrying VIP transactions
        //
        for _i in 0..vip_transactions_num {
            let mut tx = Transaction::create_vip_transaction(public_key, vip_amount);
            tx.generate(&public_key, 0, 0);
            tx.sign(&private_key);
            block.add_transaction(tx);
        }

        {
            let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
            // we have added VIP, so need to regenerate the merkle-root
            block.merkle_root =
                block.generate_merkle_root(configs.is_browser(), configs.is_spv_mode());
        }
        block.generate();
        block.sign(&private_key);

        assert!(verify_signature(
            &block.pre_hash,
            &block.signature,
            &block.creator,
        ));

        // and add first block to blockchain
        self.add_block(block).await;
    }
}

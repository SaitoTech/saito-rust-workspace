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

use crate::config::TestConfiguration;
use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};
use saito_core::core::data::crypto::{
    generate_keys, generate_random_bytes, hash, verify_signature,
};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::MiningEvent;
use saito_core::{lock_for_read, lock_for_write};

use crate::utils::load_blocks_disk;

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

pub fn create_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as Timestamp
}

//struct to manage setup of chain
pub struct ChainRunner {
    pub mempool: Arc<RwLock<Mempool>>,
    pub blockchain: Arc<RwLock<Blockchain>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub latest_block_hash: SaitoHash,
    pub network: Network,
    pub storage: Storage,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub receiver_in_miner: Receiver<MiningEvent>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl ChainRunner {
    pub fn new() -> Self {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);
        let _public_key = wallet.public_key.clone();
        let _private_key = wallet.private_key.clone();
        let peers = Arc::new(RwLock::new(PeerCollection::new()));
        let wallet_lock = Arc::new(RwLock::new(wallet));
        let blockchain = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let mempool = Arc::new(RwLock::new(Mempool::new(wallet_lock.clone())));

        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(10);
        let configs = Arc::new(RwLock::new(TestConfiguration {}));

        Self {
            wallet_lock: wallet_lock.clone(),
            blockchain,
            mempool,
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

    //load blocks via id
    //this is just the vector of blocks
    pub async fn get_blocks_vec(&self) -> Vec<Block> {
        let mut blocks = Vec::new();

        let (blockchain, _blockchain_) = lock_for_read!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);

        let latest_id = blockchain.get_latest_block_id();
        for i in 1..=latest_id {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            //println!("WINDING ID HASH - {} {:?}", i, block_hash);
            let block = blockchain.get_block(&block_hash).unwrap().clone();
            blocks.push(block);
        }

        blocks
    }

    pub async fn load_blocks_from_path(&mut self, directory_path: &str) {
        //TODO put util in core/storage or use existing one here
        let blocks_result = load_blocks_disk(&directory_path);

        blocks_result.as_ref().unwrap_or_else(|e| {
            eprintln!("Error reading blocks: {}", e);
            std::process::exit(1);
        });
        {
            let blocks = blocks_result.unwrap();
            let (mut mempool, _mempool_) = lock_for_write!(self.mempool, LOCK_ORDER_MEMPOOL);

            debug!("got blocks {}", blocks.len());
            for mut block in blocks {
                block.force_loaded = true;
                block.generate();
                debug!("block : {:?} loaded from disk", hex::encode(block.hash));
                mempool.add_block(block);
            }
        }

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);

        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);

        debug!("add_blocks_from_mempool");
        let updated = blockchain
            .add_blocks_from_mempool(
                self.mempool.clone(),
                &self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
                configs.deref(),
            )
            .await;

        debug!("updated {}", updated);
    }

    pub async fn create_txs(
        &self,
        parent_hash: SaitoHash,
        timestamp: Timestamp,
        txs_number: usize,
        txs_amount: Currency,
        txs_fee: Currency,
    ) -> Vec<Transaction> {
        let mut transactions: Vec<Transaction> = Vec::new();
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
            //transactions.insert(transaction.signature, transaction);
            transactions.push(transaction);
        }

        transactions
    }

    pub async fn create_block(
        &mut self,
        parent_hash: SaitoHash,
        transactions: Vec<Transaction>,
        timestamp: u64,
    ) -> Block {
        let mut transactions: AHashMap<SaitoSignature, Transaction> = Default::default();
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;

        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);
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
}

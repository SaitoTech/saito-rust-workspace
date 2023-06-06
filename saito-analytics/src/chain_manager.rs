
use std::sync::Arc;
use tokio::sync::RwLock;

use saito_core::core::data::wallet::Wallet;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};
use saito_core::core::data::crypto::{generate_keys, generate_random_bytes, hash, verify_signature};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::mining_thread::MiningEvent;
use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, Timestamp,
    UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET,
};

//use stub_iohandler::TestIOHandler;

//use crate::stub_iohandler::test_function;


pub struct ChainManager {
    pub mempool_lock: Arc<RwLock<Mempool>>,
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub latest_block_hash: SaitoHash,
    // pub network: Network,
    // pub storage: Storage,
    // pub peers: Arc<RwLock<PeerCollection>>,
    // pub sender_to_miner: Sender<MiningEvent>,
    // pub receiver_in_miner: Receiver<MiningEvent>,
    // pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl ChainManager {
    pub fn new() -> Self {
        println!("create new");
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);
        let _public_key = wallet.public_key.clone();
        let _private_key = wallet.private_key.clone();
        
        let wallet_lock = Arc::new(RwLock::new(wallet));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(Arc::clone(&wallet_lock))));
        let mempool_lock = Arc::new(RwLock::new(Mempool::new(Arc::clone(&wallet_lock))));
        
        Self {            
            wallet_lock: Arc::clone(&wallet_lock),
            blockchain_lock: blockchain_lock,
            mempool_lock: mempool_lock,
            latest_block_hash: [0; 32],
        }
    }
}


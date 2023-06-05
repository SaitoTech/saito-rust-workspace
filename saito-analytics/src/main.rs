// saito analytics
// run separate tool

use std::io::prelude::*;
use std::path::Path;
use std::io::{Error, ErrorKind};
use std::fs;
use std::io::{self, Read};
use tokio::sync::RwLock;
use log::{debug, error, info, trace, warn};
use std::sync::Arc;
use std::fmt::Debug;
use async_trait::async_trait;
use std::fmt;
use saito_core::core::data::crypto::{generate_keys, generate_random_bytes, hash, verify_signature};

use saito_core::common::interface_io::{InterfaceEvent, InterfaceIO};
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::network::Network;
use saito_core::common::defs::{PeerIndex, SaitoHash, BLOCK_FILE_EXTENSION};
use saito_core::core::data::peer_service::PeerService;
use saito_core::core::data::wallet::Wallet;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};

pub struct TestIOHandler {}

impl TestIOHandler {
    pub fn new() -> TestIOHandler {
        TestIOHandler {}
    }
}

impl fmt::Debug for TestIOHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TestIOHandler")
    }
}

#[async_trait]
impl InterfaceIO for TestIOHandler {
    // Implementations of the methods here...
    // Use `todo!()` for the body of the methods.
    // This macro panics with a message of "not yet implemented"
    // You should replace each `todo!()` with real implementations later
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    async fn send_message_to_all(
        &self,
        _buffer: Vec<u8>,
        _peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        debug!("send message to all");

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: PeerConfig) -> Result<(), Error> {
        debug!("connecting to peer : {:?}", peer.host);

        Ok(())
    }

    async fn disconnect_from_peer(&mut self, _peer_index: u64) -> Result<(), Error> {
        todo!("")
    }

    async fn fetch_block_from_peer(
        &self,
        _block_hash: SaitoHash,
        _peer_index: u64,
        _url: String,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        todo!()
    }
    async fn is_existing_file(&self, key: String) -> bool {
        todo!()
    }

    async fn remove_value(&self, key: String) -> Result<(), Error> {
        todo!()
    }

    fn get_block_dir(&self) -> String {
       todo!()
    }

    async fn process_api_call(
        &self,
        _buffer: Vec<u8>,
        _msg_index: u32,
        _peer_index: PeerIndex,
    ) {
        todo!()
    }

    async fn process_api_success(
        &self,
        _buffer: Vec<u8>,
        _msg_index: u32,
        _peer_index: PeerIndex,
    ) {
        todo!()
    }

    async fn process_api_error(
        &self,
        _buffer: Vec<u8>,
        _msg_index: u32,
        _peer_index: PeerIndex,
    ) {
        todo!()
    }

    fn send_interface_event(&self, _event: InterfaceEvent) {}

    async fn save_wallet(&self, wallet: &mut Wallet) -> Result<(), Error> {
        todo!()
    }

    async fn load_wallet(&self, wallet: &mut Wallet) -> Result<(), Error> {
        todo!()
    }

    async fn save_blockchain(&self) -> Result<(), Error> {
        todo!()
    }

    async fn load_blockchain(&self) -> Result<(), Error> {
        todo!()
    }

    fn get_my_services(&self) -> Vec<PeerService> {
        todo!()
    }
    
}

struct TestConfiguration {}

impl Debug for TestConfiguration {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Configuration for TestConfiguration {
    fn get_server_configs(&self) -> Option<&Server> {
        todo!()
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        todo!()
    }

    fn get_block_fetch_url(&self) -> String {
        todo!()
    }

    fn is_spv_mode(&self) -> bool {
        false
    }

    fn is_browser(&self) -> bool {
        false
    }

    fn replace(&mut self, _config: &dyn Configuration) {
        todo!()
    }
}

fn analyseblock(path: String)  {
    println!("\n >>> read block from disk");
    //println!("File: {}", path);

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        },
    };

    let deserialized_block = Block::deserialize_from_net(bytes).unwrap();
    //println!("deserialized_block {:?}" , deserialized_block);
    println!("id {:?}" , deserialized_block.id);
    println!("timestamp {:?}" , deserialized_block.timestamp);
    println!("transactions: {}" , deserialized_block.transactions.len());
    
    //println!("{}" , deserialized_block.asReadableString());
    // let tx = &deserialized_block.transactions[0];
    // println!("{:?}" , tx);
    // println!("{:?}" , tx.timestamp);
    // println!("amount: {:?}" , tx.to[0].amount);
    
}

//read block directory and analyse each file
fn analyseDir() -> std::io::Result<()> {
    
    let directory_path = "../saito-rust/data/blocks";

    for entry in fs::read_dir(directory_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            match path.into_os_string().into_string() {
                Ok(path_string) => {
                    analyseblock(path_string);                    
                }
                Err(_) => println!("Path contains non-unicode characters"),
            }
        }
    }

    Ok(())
}


fn main() {
    info!("**** Saito analytics ****");
    //analyseDir();

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

    //TODO create empty network
    let net = Network::new(
        Box::new(TestIOHandler::new()),
        peers.clone(),
        wallet_lock.clone(),
        configs.clone(),
    );
}

use async_trait::async_trait;
use std::borrow::BorrowMut;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;
use std::io::Error;
use std::path::Path;
use std::fmt::{Debug, Formatter};
use std::fmt::{Result};
use std::fmt;

use ahash::AHashMap;
use log::{debug, info};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;

use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, Timestamp,
    UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET,
};
use saito_core::common::test_io_handler::test::TestIOHandler;
use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};
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
//
use saito_core::common::interface_io::{InterfaceEvent, InterfaceIO};
use saito_core::common::defs::{PeerIndex, SaitoHash, BLOCK_FILE_EXTENSION};


pub struct TestIOHandler {}

pub impl TestIOHandler {
    pub fn new() -> TestIOHandler {
        TestIOHandler {}
    }
}

impl fmt::Debug for TestIOHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //write!(f, "TestIOHandler")
    }
}

pub fn test_function() {
    println!("This is a test function in stub_iohandler.rs");
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

    // fn get_my_services(&self) -> Vec<PeerService> {
    //     todo!()
    // }
    
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

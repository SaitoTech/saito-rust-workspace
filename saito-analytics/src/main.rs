// saito analytics
// run separate tool

use std::io::prelude::*;
use std::path::Path;
use std::io::{Error, ErrorKind};
use std::fs;
use std::io::{self, Read};
//use tokio::sync::RwLock;
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
    analyseDir();

}

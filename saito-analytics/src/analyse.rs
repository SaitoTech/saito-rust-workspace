// saito analytics
// run separate tool
mod manager;


use std::io::prelude::*;
use std::path::Path;
use std::io::{Error, ErrorKind};
use std::fs;
use std::io::{self, Read};
use std::sync::Arc;
use ahash::AHashMap;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
use tokio::sync::RwLock;

use log::{debug, error, info, trace, warn};

use saito_core::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey,
    Timestamp, UtxoSet, GENESIS_PERIOD, MAX_STAKER_RECURSION,
};


fn read_block(path: String) -> io::Result<Block> {
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return Err(e);
        },
    };

    let deserialized_block = Block::deserialize_from_net(bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(deserialized_block)
}

fn analyse_block(block: Block)  {
    //println!("deserialized_block {:?}" , deserialized_block);
    println!("id {:?}" , block.id);
    println!("timestamp {:?}" , block.timestamp);
    println!("transactions: {}" , block.transactions.len());
    
    //println!("{}" , deserialized_block.asReadableString());
    // let tx = &deserialized_block.transactions[0];
    // println!("{:?}" , tx);
    // println!("{:?}" , tx.timestamp);
    // println!("amount: {:?}" , tx.to[0].amount);
    
}

fn get_first() -> io::Result<String> {
    let directory_path = "../saito-rust/data/blocks";

    let entries = fs::read_dir(directory_path)?;

    for entry in entries {
        let entry = entry?;

        if entry.path().is_file() {
            match entry.path().into_os_string().into_string() {
                Ok(path_string) => {
                    return Ok(path_string);
                }
                Err(_) => println!("Path contains non-unicode characters"),
            }
        }
    }

    Err(io::Error::new(ErrorKind::Other, "No files found"))
}

//read block directory as block vector
fn get_blocks() -> io::Result<Vec<Block>> {

    let directory_path = "../saito-rust/data/blocks";
    let mut blocks = Vec::new();

    for entry in fs::read_dir(directory_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            match path.into_os_string().into_string() {
                Ok(path_string) => {
                    match read_block(path_string) {
                        Ok(block) => blocks.push(block),
                        Err(e) => {
                            eprintln!("Failed to read block: {}", e);
                            continue;
                        },
                    };
                }
                Err(_) => println!("Path contains non-unicode characters"),
            }
        }
    }

    Ok(blocks)
}


fn runAnalytics() {
    println!("**** Saito analytics ****");

    //let blocks: AHashMap<SaitoHash, Block>;

    //get blocks from directory

    match get_blocks() {
        Ok(blocks) => {
            println!("Got {} blocks", blocks.len());
            for block in blocks {
                // Now you can analyse each block
                analyse_block(block);
            }
        }
        Err(e) => {
            eprintln!("Error reading blocks: {}", e);
        },
    };

    //analyseDir();

    //get the first block read

    let keys = generate_keys();

    let mut t = TestManager::new();

    // let first_block_path = match get_first() {
    //     Ok(path) => path,
    //     Err(e) => {
    //         eprintln!("Failed to get first block path: {}", e);
    //         return;
    //     },
    // };

    // let first_block = match read_block(first_block_path) {
    //     Ok(block) => block,
    //     Err(e) => {
    //         eprintln!("Failed to read first block: {}", e);
    //         return;
    //     },
    // };

    // println!("{:?}", first_block);

    let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
    let blockchain = Blockchain::new(wallet);
    assert_eq!(blockchain.fork_id, [0; 32]);
    assert_eq!(blockchain.genesis_block_id, 0);

    
    //let block1 = blockchain.get_latest_block().unwrap();
    //println!("{:?}", block1.id);
    //println!("{:?}", block1.hash);

    // blockchain
    //             .add_block(
    //                 block,
    //                 None,
    //                 None,
    //                 None,
    //                 None,
    //                 None,
    //             );
}

// saito analytics
// run separate tool

use std::cmp::Ordering;
use std::fs;
use std::io::prelude::*;
use std::io::{self, Read};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use ahash::AHashMap;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
use tokio::sync::RwLock;

use log::{debug, error, info, trace, warn};

use saito_core::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey,
    Timestamp, UtxoSet, GENESIS_PERIOD, MAX_STAKER_RECURSION,
};

pub fn read_block(path: String) -> io::Result<Block> {
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return Err(e);
        }
    };

    let deserialized_block = Block::deserialize_from_net(bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(deserialized_block)
}

//read block directory as block vector
pub fn get_blocks(directory_path: &str) -> io::Result<Vec<Block>> {
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
                        }
                    };
                }
                Err(_) => println!("Path contains non-unicode characters"),
            }
        }
    }

    blocks.sort_by(|a, b| {
        if a.id < b.id {
            Ordering::Less
        } else if a.id > b.id {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });

    Ok(blocks)
}

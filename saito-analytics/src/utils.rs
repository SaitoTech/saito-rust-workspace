// utilities

use std::cmp::Ordering;
use std::fs;
use std::io::prelude::*;
use std::io::{self, Read};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;

use log::{debug, error, info, trace, warn};
use saito_core::common::defs::push_lock;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::slip::Slip;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::transaction::TransactionType;
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::RwLock;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrettyBlock {
    pub id: u64,
    pub timestamp: Timestamp,
    pub previous_block_hash: String,
    //pub creator: string,
    //pub merkle_root: string,
    // pub signature: string,
    // pub treasury: Currency,
    // pub burnfee: Currency,
    // pub difficulty: u64,
    // pub staking_treasury: Currency,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrettyTx {
    pub timestamp: Timestamp,
    //pub from: Vec<Slip>,
    //pub to: Vec<Slip>,
    pub transaction_type: TransactionType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrettySlip {
    pub public_key: String,
    pub amount: Currency,
}

use saito_core::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey,
    Timestamp, UtxoSet, GENESIS_PERIOD, LOCK_ORDER_WALLET, MAX_STAKER_RECURSION,
};

pub fn bytes_to_hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

pub fn pretty_print_block(block: &Block) -> Result<(), serde_json::Error> {
    let pretty_block = PrettyBlock {
        id: block.id,
        timestamp: block.timestamp,
        previous_block_hash: bytes_to_hex_string(&block.previous_block_hash),
        //TODO more fields
        //creator: block.creator.iter().map(|byte| format!("{:02x}", byte)).collect(),
        //merkle_root: block.merkle_root.iter().map(|byte| format!("{:02x}", byte)).collect(),
        //merkle_root: string,
        //signature: string,
        //treasury: Currency,
    };

    let block_string = serde_json::to_string_pretty(&pretty_block)?;
    println!(">>> block >>>>>>>>>>> ");
    println!("{}", block_string);

    println!("--- tx -------");
    for tx in &block.transactions {
        pretty_print_tx(&tx);
    }

    Ok(())
}

fn pretty_print_blocks(directory_path: String) {
    let blocks_result = load_blocks_disk(&directory_path);

    match blocks_result.as_ref() {
        Ok(blocks) => {
            println!("Got {} blocks", blocks.len());
            for block in blocks {
                if let Err(e) = pretty_print_block(&block) {
                    eprintln!("Error pretty printing block: {}", e);
                }
            }
        }
        Err(e) => {
            //eprintln!("Error reading blocks: {}", e);
            eprintln!("Error ");
        }
    };
}

pub fn pretty_print_tx(tx: &Transaction) -> Result<(), serde_json::Error> {
    let pretty_tx = PrettyTx {
        timestamp: tx.timestamp,
        transaction_type: tx.transaction_type,
    };

    let tx_string = serde_json::to_string_pretty(&pretty_tx)?;
    println!("{}", tx_string);

    println!("from {}", tx.from.len());
    println!("to {}", tx.to.len());

    Ok(())
}

pub fn pretty_print_slip(slip: &Slip) -> Result<(), serde_json::Error> {
    let pretty_slip = PrettySlip {
        amount: slip.amount,
        public_key: bytes_to_hex_string(&slip.public_key),
    };

    let slip_str = serde_json::to_string_pretty(&pretty_slip)?;
    println!("{}", slip_str);

    Ok(())
}

pub fn calc_sum_issued(block: &Block) -> u64 {
    let mut sum_issued = 0;
    for tx in &block.transactions {
        for slip in &tx.to {
            sum_issued += slip.amount;
        }
    }
    sum_issued
}

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
//TODO: should be in core?
pub fn load_blocks_disk(directory_path: &str) -> io::Result<Vec<Block>> {
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

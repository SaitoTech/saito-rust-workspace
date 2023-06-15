// general utilities, can have overlap with core or rustnode

use std::fs;
use std::io::prelude::*;
use std::io::{self, Read};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::cmp::Ordering;

use log::{debug, error, info, trace, warn};
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::transaction::TransactionType;
use saito_core::core::data::wallet::Wallet;
use saito_core::common::defs::push_lock;
use saito_core::{lock_for_read, lock_for_write};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::RwLock;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrettyBlock {
    pub id: u64,
    pub timestamp: Timestamp,
    pub previous_block_hash: String, // Note that this is a String
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

use saito_core::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey,
    Timestamp, UtxoSet, GENESIS_PERIOD, MAX_STAKER_RECURSION, LOCK_ORDER_WALLET,
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

pub fn pretty_print_tx(tx: &Transaction) -> Result<(), serde_json::Error> {
    let pretty_tx = PrettyTx {
        timestamp: tx.timestamp,
        transaction_type: tx.transaction_type,
    };

    let tx_string = serde_json::to_string_pretty(&pretty_tx)?;
    println!("{}", tx_string);

    Ok(())
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
//SHOULD BE IN CORE?
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

//COPIED
async fn generate_spammer_init_tx(wallet: Arc<RwLock<Wallet>>) {
    info!("generating spammer init transaction");

    let private_key;

    {
        let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);
        private_key = wallet.private_key;
    }

    let spammer_public_key: SaitoPublicKey =
        hex::decode("03145c7e7644ab277482ba8801a515b8f1b62bcd7e4834a33258f438cd7e223849")
            .unwrap()
            .try_into()
            .unwrap();

    {
        let mut vip_transaction =
            Transaction::create_vip_transaction(spammer_public_key, 100_000_000);
        vip_transaction.sign(&private_key);
    }
}

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
use crate::sutils::get_blocks;

fn analyse_block(block: Block) {
    //println!("deserialized_block {:?}" , deserialized_block);
    println!("id {:?}", block.id);
    println!("timestamp {:?}", block.timestamp);
    println!("transactions: {}", block.transactions.len());

    //println!("{}" , deserialized_block.asReadableString());
    // let tx = &deserialized_block.transactions[0];
    // println!("{:?}" , tx);
    // println!("{:?}" , tx.timestamp);
    // println!("amount: {:?}" , tx.to[0].amount);
}

pub fn runAnalytics() {
    //TODO comment

    println!("**** Saito analytics ****");

    //get blocks from directory
    let directory_path = "../../sampleblocks";

    let blocks_result = get_blocks(directory_path);

    match blocks_result.as_ref() {
        Ok(blocks) => {
            println!("Got {} blocks", blocks.len());
            for block in blocks {
                analyse_block(block.clone());
            }
        }
        Err(e) => {
            eprintln!("Error reading blocks: {}", e);
        }
    };

    let keys = generate_keys();

    let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
    let mut blockchain = Blockchain::new(wallet);
    assert_eq!(blockchain.fork_id, [0; 32]);
    assert_eq!(blockchain.genesis_block_id, 0);

    println!("genesis_block_id: {}", blockchain.genesis_block_id);

    //blockchain.add_block_testing(blocks_result.unwrap()[0]);

    match blocks_result.as_ref() {
        Ok(blocks) => {
            println!("blocks {}", blocks.len());
            for block in blocks {
                // Now you can analyse each block
                println!(".....");
                //block.generate();
                //blockchain.add_block_testing(block.clone());
            }
        }
        Err(e) => {
            eprintln!("Error reading blocks: {}", e);
        }
    };
}

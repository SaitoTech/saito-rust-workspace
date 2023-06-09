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
//mod sutils;
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
    println!("**** Saito analytics ****");

    //let blocks: AHashMap<SaitoHash, Block>;

    //get blocks from directory
    let directory_path = "../../sampleblocks";

    //let mut blocks = Vec::new();

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

    //analyseDir();

    //get the first block read

    let keys = generate_keys();

    //let mut t = TestManager::new();

    // println!("{:?}", first_block);

    let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
    let mut blockchain = Blockchain::new(wallet);
    assert_eq!(blockchain.fork_id, [0; 32]);
    assert_eq!(blockchain.genesis_block_id, 0);

    println!("genesis_block_id: {}", blockchain.genesis_block_id);

    //blockchain.add_block_testing(blocks_result.unwrap()[0]);

    //let blocks = blocks_result.unwrap();

    match blocks_result.as_ref() {
        Ok(blocks) => {
            println!("blocks {}", blocks.len());
            for block in blocks {
                // Now you can analyse each block
                println!(".....");
                //block.generate();
                blockchain.add_block_testing(block.clone());
            }
        }
        Err(e) => {
            eprintln!("Error reading blocks: {}", e);
        }
    };

    // match &blocks_result {
    //     Ok(blocks) => {
    //         if !blocks.is_empty() {
    //             blockchain.add_block_testing(blocks[0].clone());
    //         } else {
    //             eprintln!("No blocks to add");
    //         }
    //     },
    //     Err(e) => {
    //         eprintln!("Error reading blocks: {}", e);
    //     },
    // };

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

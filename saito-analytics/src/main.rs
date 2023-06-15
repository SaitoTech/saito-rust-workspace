// saito analytics

use ahash::AHashMap;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
use serde_json;
use std::fs;
use std::io::prelude::*;
use std::io::{self, Read};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use log::{debug, error, info, trace, warn};

use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey};

use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, Timestamp, UtxoSet, GENESIS_PERIOD, LOCK_ORDER_MEMPOOL,
    LOCK_ORDER_WALLET, MAX_STAKER_RECURSION, MIN_GOLDEN_TICKETS_DENOMINATOR,
    MIN_GOLDEN_TICKETS_NUMERATOR, PRUNE_AFTER_BLOCKS,
};
use saito_core::common::defs::{LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS};
use std::fs::File;
use std::io::Write;

mod analyse;
mod chain_manager;
mod sutils;
mod test_io_handler;

use crate::sutils::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

//take blocks from a directory and output a utxo dump file
async fn runDump() {
    //read from provided directory
    let directory_path = "../../sampleblocks";
    let tresh = 1000000;

    let mut t = chain_manager::ChainManager::new();
    t.show_info();

    let blocks_result = get_blocks(directory_path);

    match blocks_result.as_ref() {
        Ok(blocks) => {
            println!("Got {} blocks", blocks.len());
            for block in blocks {
                t.add_block(block.clone()).await;
            }
        }
        Err(e) => {
            eprintln!("Error reading blocks: {}", e);
        }
    };

    t.dump_utxoset(tresh).await;
}

fn pretty_print_blocks() {
    let directory_path = "../../sampleblocks";

    let blocks_result = get_blocks(directory_path);

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

fn sum_balances(b: &AHashMap<String, u64>) -> u64 {
    let mut sumv = 0;
    for (key, value) in b {
        sumv += value;
    }
    sumv
}

#[tokio::test]
async fn test_chain_manager() {
    // let viptx = 10;
    // let mut t = chain_manager::ChainManager::new();
    // t.initialize(viptx, 1_000_000).await;
    // t.wait_for_mining_event().await;
    // t.check_blockchain().await;

    // let v = t.get_blocks().await;
    // println!("{}", v.len());

    // let b = t.get_utxobalances(0).await;
    // println!("{}", b.len());

    // let sumv = sum_balances(&b);
    // println!("{}", sumv);
    // assert_eq!(1_000_000, sumv);
}

#[tokio::test]
async fn test_chain_manager_basic_tx() {
    let mut t = chain_manager::ChainManager::new();
    let vip_amount = 1_000_000_000;
    let vec_tx = t.generate_tx(1, vip_amount).await;
    assert_eq!(vec_tx.len(), 1);
    pretty_print_tx(&vec_tx[0]);
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("run analytics");

    let mut t = chain_manager::ChainManager::new();
    let vip_amount = 1_000_000;
    let vec_tx = t.generate_tx(1, vip_amount).await;
    pretty_print_tx(&vec_tx[0]);
    pretty_print_slip(&vec_tx[0].to[0]);

    //t.initialize(viptx, vipamount).await;

    //create vec tx
    //create one block
    //apply 

    //test apply works


    //runDump().await;

    //pretty_print_blocks();

    //static analysis
    //analyse::runAnalytics();

    //////////
    //simulated blocks
    // let viptx = 10;
    // let mut t = chain_manager::ChainManager::new();
    // t.initialize(viptx, 1_000_000_000).await;
    // t.wait_for_mining_event().await;

    // {
    //     let (blockchain, _blockchain_) = lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
    // }
    // t.check_blockchain().await;

    // println!("... get_blocks ...");

    // let v = t.get_blocks().await;
    // println!("{}", v.len());

    // let b = t.get_utxobalances(0).await;
    // println!("{}", b.len());

    // let mut sumv = sum_balances(&b);
    // println!("{}", sumv);
    //assert_eq(1_000_000_000, sumv);

    // t.dump_utxoset(20000000000).await;
    // t.dump_utxoset(10000000000).await;
    // t.dump_utxoset(0).await;

    Ok(())
}

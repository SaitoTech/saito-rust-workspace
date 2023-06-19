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
use tracing_subscriber;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use std::str::FromStr;
use tracing_subscriber::filter::LevelFilter;


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

mod calc;
mod config;
mod runner;
mod sutils;
mod test_io_handler;

//use crate::sutils::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("saito analytics");

    println!("Running saito");

    let directory_path = "../../sampleblocks";


    let mut r = runner::ChainRunner::new();
    println!("....");
    r.load_blocks_from_path(&directory_path).await;
    println!("wating");

///////////////
    // 
    // init storage
    // init mempool
    // init configs
    // init blockchain
    // init network


    //let generate_genesis_block = true;

    // storage
    //         .load_blocks_from_disk(mempool.clone())
    //         .await;

    //     let (configs, _configs_) = lock_for_read!(configs, LOCK_ORDER_CONFIGS);
    //     let (mut blockchain, _blockchain_) =
    //         lock_for_write!(self.blockchain, LOCK_ORDER_BLOCKCHAIN);


    //     blockchain
    //         .add_blocks_from_mempool(
    //             self.mempool.clone(),
    //             &self.network,
    //             &mut self.storage,
    //             self.sender_to_miner.clone(),
    //             configs.deref(),
    //         )
    //         .await;
///////////////
    
    // println!("read {} blocks from disk", blocks.len());

    // let gen_block = &blocks[0];
    // let sum_issued = calc::calc_sum_issued(&gen_block);
    // println!("sum issued {}", sum_issued);

    //init chain runner
    //apply genesis block
    //run utox calc

    // pretty_print_tx(&blocks[0].transactions[0]);
    // for slip in &blocks[0].transactions[0].from {
    //     pretty_print_slip(&slip);
    // }
    // for slip in &blocks[0].transactions[0].to {
    //     pretty_print_slip(&slip);
    // }

    Ok(())
}

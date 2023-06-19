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

    //TODO
    //utxocalc

    Ok(())
}

// saito analytics

use ahash::AHashMap;
use clap::{App, Arg};
use log::{debug, error, info, trace, warn};
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json;
use std::fs;
use std::io::prelude::*;
use std::io::{self, Read};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing_subscriber;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

use bs58;
use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, Timestamp, UtxoSet, GENESIS_PERIOD, LOCK_ORDER_MEMPOOL,
    LOCK_ORDER_WALLET, MAX_STAKER_RECURSION, MIN_GOLDEN_TICKETS_DENOMINATOR,
    MIN_GOLDEN_TICKETS_NUMERATOR, PRUNE_AFTER_BLOCKS,
};
use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey};
use saito_core::common::defs::{LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS};
use std::fs::File;
use std::io::Write;

mod config;
mod runner;
mod test_io_handler;
mod utils;

use utils::pretty_print_block;

pub fn create_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as Timestamp
}

fn setup_logging() {
    let mut filter = EnvFilter::from_default_env();

    let directives = vec![
        "tokio_tungstenite",
        "tungstenite",
        "mio::poll",
        "hyper::proto",
        "hyper::client",
        "want",
        "reqwest::async_impl",
        "reqwest::connect",
        "warp::filters",
    ];

    for directive in directives {
        filter = filter.add_directive(Directive::from_str(&format!("{}=info", directive)).unwrap());
    }

    let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);

    tracing_subscriber::registry().with(fmt_layer).init();
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("saito analytics");

    let default_path = "../../sampleblocks";
    let utxodump_file = "utxoset.dat";

    setup_logging();

    let mut r = runner::ChainRunner::new();

    //utxocalc based on sample blocks
    //run check on static path

    fn is_u64(val: String) -> Result<(), String> {
        match val.parse::<u64>() {
            Ok(_) => Ok(()),
            Err(_) => Err(String::from("This value must be a valid u64")),
        }
    }

    let matches = App::new("Saito")
        .arg(
            Arg::with_name("blockdir")
                .long("blockdir")
                .value_name("BLOCKDIR")
                .help("Sets a custom block path")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("threshold")
                .long("threshold")
                .value_name("THRESHOLD")
                .help("Sets a custom threshold")
                .takes_value(true)
                .default_value("1")
                .validator(|v| match v.parse::<u64>() {
                    Ok(_) => Ok(()),
                    Err(_) => Err(String::from("This value must be a valid u64")),
                }),
        )
        .get_matches();

    let threshold: u64 = matches
        .value_of("threshold")
        .unwrap_or("1")
        .parse()
        .unwrap();

    let directory_path = matches.value_of("blockdir").unwrap_or(default_path);

    r.load_blocks_from_path(&directory_path).await;

    type UtxoSetBalance = AHashMap<SaitoPublicKey, u64>;
    let mut utxo_balances: UtxoSetBalance = AHashMap::new();

    info!("run dump utxoset. take blocks from {}", directory_path);

    let blocks = r.get_blocks_vec().await;

    //get total output of first block
    let firstblock = &blocks[0];
    let mut inital_out = 0;
    for j in 0..firstblock.transactions.len() {
        let tx = &firstblock.transactions[j];

        tx.to.iter().for_each(|output| {
            inital_out += output.amount;
        });
    }

    info!("inital supply: {}", inital_out);

    //assume longest chain
    let input_slip_spendable = false;
    let output_slip_spendable = true;

    //iterate through all blocks and tx
    for block in blocks {
        info!("block {}", block.id);
        for j in 0..block.transactions.len() {
            let tx = &block.transactions[j];

            tx.from.iter().for_each(|input| {
                utxo_balances
                    .entry(input.public_key)
                    .and_modify(|e| *e -= input.amount)
                    .or_insert(0);
            });

            tx.to.iter().for_each(|output| {
                utxo_balances
                    .entry(output.public_key)
                    .and_modify(|e| *e += output.amount)
                    .or_insert(output.amount);
            });
        }
    }

    let mut total_value = 0;
    for (key, value) in &utxo_balances {
        if value > &0 {
            total_value += value;
        }
    }
    info!("total_value {}", total_value);

    assert_eq!(total_value, inital_out);

    let file_path = format!("data/{}", utxodump_file);
    let mut file = File::create(file_path).unwrap();

    let (mut blockchain, _blockchain_) = lock_for_write!(r.blockchain, LOCK_ORDER_BLOCKCHAIN);

    //if we want to add type need to check on pub slip_type: SlipType,
    //write header
    writeln!(
        file,
        "UTXO state height: latest_block_id {}",
        blockchain.get_latest_block_id()
    );

    let txtype = "normal";

    for (key, value) in &utxo_balances {
        if value > &threshold {
            let key_base58 = bs58::encode(key).into_string();

            writeln!(file, "{}\t{}\t{}", value, key_base58, txtype);
        }
    }

    Ok(())
}

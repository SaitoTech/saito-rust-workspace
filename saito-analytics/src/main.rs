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
use tracing_subscriber::Layer;

use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, Timestamp, UtxoSet, GENESIS_PERIOD, LOCK_ORDER_MEMPOOL,
    LOCK_ORDER_WALLET, MAX_STAKER_RECURSION, MIN_GOLDEN_TICKETS_DENOMINATOR,
    MIN_GOLDEN_TICKETS_NUMERATOR, PRUNE_AFTER_BLOCKS,
};
use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey};
use saito_core::common::defs::{LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS};
use std::fs::File;
use std::io::Write;

mod calc;
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

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("saito analytics");

    let filter = tracing_subscriber::EnvFilter::from_default_env();
    let filter = filter.add_directive(Directive::from_str("tokio_tungstenite=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("tungstenite=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("mio::poll=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("hyper::proto=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("hyper::client=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("want=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("reqwest::async_impl=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("reqwest::connect=info").unwrap());
    let filter = filter.add_directive(Directive::from_str("warp::filters=info").unwrap());

    let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);

    tracing_subscriber::registry().with(fmt_layer).init();

    let mut r = runner::ChainRunner::new();

    //utxocalc based on sample blocks
    //run check on static path

    let matches = App::new("Saito")
        .arg(
            Arg::with_name("blockdir")
                .long("blockdir")
                .value_name("BLOCKDIR")
                .help("Sets a custom block path")
                .takes_value(true),
        )
        .get_matches();

    let directory_path = matches.value_of("blockdir").unwrap_or("../../sampleblocks");
    //let directory_path = ;
    let utxodump_file = "utxoset.dat";

    r.load_blocks_from_path(&directory_path).await;

    let mut utxoset: UtxoSet = AHashMap::new();

    type UtxoSetBalance = AHashMap<SaitoUTXOSetKey, u64>;
    let mut utxo_balances: UtxoSetBalance = AHashMap::new();

    info!("run dump utxoset. take blocks from {}", directory_path);

    let blocks = r.get_blocks_vec().await;

    pretty_print_block(&blocks[0]);

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
                utxoset.insert(input.utxoset_key, input_slip_spendable);

                utxo_balances
                    .entry(input.utxoset_key)
                    .and_modify(|e| *e -= input.amount)
                    .or_insert(0);
            });

            tx.to.iter().for_each(|output| {
                utxoset.insert(output.utxoset_key, output_slip_spendable);

                utxo_balances
                    .entry(output.utxoset_key)
                    .and_modify(|e| *e += output.amount)
                    .or_insert(output.amount);
            });
        }
    }

    let mut total_value = 0;
    for (key, value) in &utxo_balances {
        if value > &0 {
            info!("{:?} {:?}", key, value);
            total_value += value;
        }
    }
    info!("total_value {}", total_value);

    //should be equal
    info!("{}", total_value == inital_out);

    let file_path = format!("data/{}", utxodump_file);
    let mut file = File::create(file_path).unwrap();

    let (mut blockchain, _blockchain_) = lock_for_write!(r.blockchain, LOCK_ORDER_BLOCKCHAIN);

    writeln!(
        file,
        "UTXO state height: latest_block_id {}",
        blockchain.get_latest_block_id()
    );

    let threshold = 1;
    //TODO
    let txtype = "normal";

    //output should be
    //TODO public key compressed
    //25000	21ronA4HFRaoqJdPt1fZQ6rz7SS5TKAyr3QzN429miBZA	VipOutput

    for (key, value) in &utxo_balances {
        if value > &threshold {
            let key_hex = hex::encode(key);
            println!("{}\t{}", key_hex, value);
            writeln!(file, "{}\t{:?}\t{}", key_hex, value, txtype);
        }
    }

    Ok(())
}

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
    // let filter = filter.add_directive(Directive::from_str("saito_stats=info").unwrap());

    let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);

    tracing_subscriber::registry().with(fmt_layer).init();

    let mut r = runner::ChainRunner::new();
    println!("....");
    r.load_blocks_from_path(&directory_path).await;

    //utxocalc

    let mut utxoset: UtxoSet = AHashMap::new();

    type UtxoSetBalance = AHashMap<SaitoUTXOSetKey, u64>;
    let mut utxo_balances: UtxoSetBalance = AHashMap::new();

    info!("---- utxoset ");

    let blocks = r.get_blocks_vec().await;
    //this call the reorg chain here, simpler

    //get total output of first block
    let firstblock = &blocks[0];
    let mut inital_out = 0;
    for j in 0..firstblock.transactions.len() {
        let tx = &firstblock.transactions[j];
        // tx.from.iter().for_each(|input| {
        // });

        tx.to.iter().for_each(|output| {
            inital_out += output.amount;
        });
    }

    println!("inital_out {}", inital_out);

    //assume longest chain
    let input_slip_spendable = false;
    let output_slip_spendable = true;

    for block in blocks {
        println!("block {}", block.id);
        for j in 0..block.transactions.len() {
            let tx = &block.transactions[j];
            //println!("from {}", tx.from.len());
            //println!("to {}", tx.to.len());

            // //block.transactions[j].on_chain_reorganization(&mut utxoset, true, block.id);
            //will do this
            tx.from.iter().for_each(|input| {
                // if self.amount > 0 {
                utxoset.insert(input.utxoset_key, input_slip_spendable);

                utxo_balances
                    .entry(input.utxoset_key)
                    .and_modify(|e| *e -= input.amount)
                    .or_insert(0);

                
            });

            tx.to.iter().for_each(|output| {
                // if self.amount > 0 {
                utxoset.insert(output.utxoset_key, output_slip_spendable);
                
                utxo_balances
                    .entry(output.utxoset_key)
                    .and_modify(|e| *e += output.amount)
                    .or_insert(output.amount);
             
            });
        }
    }

    // for (key, value) in utxoset {
    //     println!("{:?} {:?}", key, value);
    //     // match utxoset.get(key) {
    // }

    let mut total_value = 0;
    for (key, value) in utxo_balances {
        if (value > 0) {
            println!("{:?} {:?}", key, value);
            total_value += value;
        }
        // match utxoset.get(key) {
    }
    println!("total_value {}", total_value);
    //100_000_000

    //should be equal
    println!("{}", total_value==inital_out);

    Ok(())
}

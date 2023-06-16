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

mod chain_manager;
mod cli;
mod sutils;
mod test_io_handler;

use crate::sutils::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

fn main() {
    println!("saito analytics");

    //cli::runAnalytics(directory_path.to_string());

    let directory_path = "../../sampleblocks";
    let blocks_result = get_blocks(&directory_path);

    blocks_result
        .as_ref()
        .map(|blocks| {
            println!("Got {} blocks", blocks.len());
            //pretty_print_block(&blocks[0]);
            pretty_print_tx(&blocks[0].transactions[0]);
            for slip in &blocks[0].transactions[0].from {
                pretty_print_slip(&slip);
            }
            for slip in &blocks[0].transactions[0].to {
                pretty_print_slip(&slip);
            }
            // for block in blocks {
            //     if let Err(e) = pretty_print_block(&block) {
            //         eprintln!("Error pretty printing block: {}", e);
            //     }
            // }
        })
        .map_err(|e| {
            eprintln!("Error reading blocks: {}", e);
        });
}

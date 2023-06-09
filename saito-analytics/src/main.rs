// saito analytics

use ahash::AHashMap;
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
use saito_core::core::data::crypto::generate_keys;
use saito_core::core::data::wallet::Wallet;
use saito_core::{lock_for_read, lock_for_write};
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

use crate::sutils::get_blocks;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //static analysis
    //analyse::runAnalytics();

    //need to add type

    let mut t = chain_manager::ChainManager::new();
    t.show_info();

    //read from provided directory
    let directory_path = "../../sampleblocks";
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

    t.dump_utxoset(0).await;

    //////////
    //simulated blocks
    //let viptx = 10;
    //t.initialize(viptx, 1_000_000_000).await;
    //t.wait_for_mining_event().await;

    // {
    //     let (blockchain, _blockchain_) = lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
    // }
    //t.check_blockchain().await;
    
    //t.dump_utxoset(20000000000).await;

    Ok(())
}

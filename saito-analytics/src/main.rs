// saito analytics
// run separate tool
//mod manager;

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
mod test_io_handler;
mod chain_manager;

use serde::{Serialize, Deserialize, Serializer, Deserializer};
//use serde::de::Error;



// fn write_utxo_to_file(blockchain: &Blockchain) -> std::io::Result<()> {
//     //let serialized = serde_json::to_string(&blockchain.utxoset)?;

//     //let mut file = File::create("utxoset.json")?;
//     //file.write_all(serialized.as_bytes())?;

//     let mut file = File::create("utxoset.dat")?;

//     for (key, value) in &blockchain.utxoset {
//         //file.write_all(&(*key).0)?;
//         //file.write_all(&[*value as u8])?;
        
//         file.write_all(&(*key))?;
//         //file.write_all(&[*value as u8])?;
//     }

//     Ok(())
// }


#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //static analysis
    //analyse::runAnalytics();

    //TODO take in blocks from read_blocks    

    let mut t = chain_manager::ChainManager::new();
    t.show_info();
    let viptx = 10;
    t.initialize(viptx, 1_000_000_000).await;
    t.wait_for_mining_event().await;

    {
        let (blockchain, _blockchain_) = lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);        
    }
    //t.check_blockchain().await;

    t.dump_utxoset().await;

    //t.check_token_supply().await;
    //t.check_utxo().await;

    //t.dump_utxoset(100).await;

    //////

    //t.check_token_supply().await;

    // let mut block1;
    // let mut block1_id;
    // let mut block1_hash;
    // let mut ts;

    // t.initialize_with_timestamp(100, 1_000_000_000, 10_000_000);

    // for _i in (0..20).step_by(1) {
    //     {
    //         let (blockchain, _blockchain_) =
    //             lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

    //         block1 = blockchain.get_latest_block().unwrap();
    //         block1_hash = block1.hash;
    //         block1_id = block1.id;
    //         ts = block1.timestamp;
    //     }
    // }

    Ok(())
}

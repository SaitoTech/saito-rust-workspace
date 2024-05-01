use rayon::prelude::*;
use std::fs::File;
use std::io::{Error, ErrorKind, Write};
use std::sync::Arc;

use ahash::AHashMap;
use bs58;
use log::{debug, error, warn};
use tokio::sync::RwLock;

use crate::core::consensus::block::{Block, BlockType};
use crate::core::consensus::mempool::Mempool;
use crate::core::consensus::slip::{Slip, SlipType};
use crate::core::defs::{PrintForLog, SaitoPublicKey, PROJECT_PUBLIC_KEY};
use crate::core::io::interface_io::InterfaceIO;

#[derive(Debug)]
pub struct Storage {
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
}

pub const ISSUANCE_FILE_PATH: &'static str = "./data/issuance/issuance";
pub const EARLYBIRDS_FILE_PATH: &'static str = "./data/issuance/earlybirds";
pub const DEFAULT_FILE_PATH: &'static str = "./data/issuance/default";
pub const UTXOSTATE_FILE_PATH: &'static str = "./data/issuance/utxodata";

pub struct StorageConfigurer {}

pub fn configure_storage() -> String {
    if cfg!(test) {
        String::from("./data/test/blocks/")
    } else {
        String::from("./data/blocks/")
    }
}

impl Storage {
    pub fn new(io_interface: Box<dyn InterfaceIO + Send + Sync>) -> Storage {
        Storage { io_interface }
    }
    /// read from a path to a Vec<u8>
    pub async fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let buffer = self.io_interface.read_value(path).await;
        if buffer.is_err() {
            let err = buffer.err().unwrap();
            error!("reading failed : {:?}", err);
            return Err(err);
        }
        let buffer = buffer.unwrap();
        Ok(buffer)
    }

    pub async fn write(&mut self, data: &[u8], filename: &str) {
        self.io_interface
            .write_value(filename, data)
            .await
            .expect("writing to storage failed");
    }

    pub async fn file_exists(&self, filename: &str) -> bool {
        self.io_interface.is_existing_file(filename).await
    }

    pub fn generate_block_filepath(&self, block: &Block) -> String {
        self.io_interface.get_block_dir() + block.get_file_name().as_str()
    }
    pub async fn write_block_to_disk(&mut self, block: &Block) -> String {
        let buffer = block.serialize_for_net(BlockType::Full);
        let filename = self.generate_block_filepath(block);

        let result = self
            .io_interface
            .write_value(filename.as_str(), buffer.as_slice())
            .await;
        if result.is_err() {
            let err = result.err().unwrap();
            // TODO : panicking currently to make sure we can serve any blocks for which we have propagated the header for
            panic!("failed writing block to disk. {:?}", err);
        }
        filename
    }

    pub async fn load_block_name_list(&self) -> Result<Vec<String>, Error> {
        let block_dir_path = self.io_interface.get_block_dir();

        match self
            .io_interface
            .ensure_block_directory_exists(block_dir_path.as_str())
        {
            Ok(()) => debug!("Block directory created"),
            Err(err) => {
                error!("Error creating block directory {:?}", err);
                return Err(Error::from(err));
            }
        }

        let list = self.io_interface.load_block_file_list().await;
        if list.is_err() {
            error!(
                "failed loading block list from disk : {:?}",
                list.err().unwrap()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }
        let mut list = list.unwrap();
        list.sort();
        Ok(list)
    }

    pub async fn load_blocks_from_disk(
        &mut self,
        file_names: &[String],
        mempool_lock: Arc<RwLock<Mempool>>,
    ) {
        debug!("loading  {:?} blocks from disk", file_names.len());

        let mut mempool = mempool_lock.write().await;
        for (_index, file_name) in file_names.iter().enumerate() {
            let file_name = file_name.clone();
            let result = self
                .io_interface
                .read_value((self.io_interface.get_block_dir() + file_name.as_str()).as_str())
                .await;
            if result.is_err() {
                error!(
                    "failed loading block from disk : {:?}",
                    result.err().unwrap()
                );
                return;
            }
            debug!("file : {:?} loaded", file_name);
            let buffer: Vec<u8> = result.unwrap();
            let buffer_len = buffer.len();
            let result = Block::deserialize_from_net(buffer);
            if result.is_err() {
                // ideally this shouldn't happen since we only write blocks which are valid to disk
                warn!(
                    "failed deserializing block with buffer length : {:?}",
                    buffer_len
                );
                return;
            }
            let mut block: Block = result.unwrap();
            block.force_loaded = true;
            block.generate();
            debug!("block : {:?} loaded from disk", block.hash.to_hex());
            mempool.add_block(block);
        }
        // mempool.blocks_queue.shrink_to_fit();
        // mempool.transactions.shrink_to_fit();
        // mempool.golden_tickets.shrink_to_fit();

        debug!("blocks loaded to mempool");
    }

    pub async fn load_block_from_disk(&self, file_name: &str) -> Result<Block, std::io::Error> {
        debug!("loading block {:?} from disk", file_name);
        let result = self.io_interface.read_value(file_name).await;
        if result.is_err() {
            error!(
                "failed loading block from disk : {:?}",
                result.err().unwrap()
            );
            return Err(Error::from(ErrorKind::NotFound));
        }
        let buffer = result.unwrap();
        Block::deserialize_from_net(buffer)
    }

    pub async fn delete_block_from_disk(&self, filename: &str) -> bool {
        self.io_interface.remove_value(filename).await.is_ok()
    }

    /// Asynchronously retrieves token issuance slips from the provided file path.
    ///
    /// This function reads a file from disk that contains the token issuance slips
    /// and returns these slips as a vector.
    pub async fn get_token_supply_slips_from_disk_path(&self, issuance_file: &str) -> Vec<Slip> {
        let mut v: Vec<Slip> = vec![];
        let mut tokens_issued = 0;
        //
        if self.file_exists(issuance_file).await {
            if let Ok(lines) = self.io_interface.read_value(issuance_file).await {
                let mut contents = String::from_utf8(lines).unwrap();
                contents = contents.trim_end_matches('\r').to_string();
                let lines: Vec<&str> = contents.split("\n").collect();

                for line in lines {
                    let line = line.trim_end_matches('\r');
                    if !line.is_empty() {
                        if let Some(mut slip) = self.convert_issuance_into_slip(line) {
                            slip.generate_utxoset_key();
                            v.push(slip);
                        }
                    }
                }

                for i in 0..v.len() {
                    tokens_issued += v[i].amount;
                }

                debug!("{:?} tokens issued", tokens_issued);
                return v;
            }
        } else {
            error!("issuance file does not exist");
        }

        return vec![];
    }

    /// get issuance slips from the standard file
    pub async fn get_token_supply_slips_from_disk(&self) -> Vec<Slip> {
        return self
            .get_token_supply_slips_from_disk_path(ISSUANCE_FILE_PATH)
            .await;
    }

    /// convert an issuance expression to slip
    fn convert_issuance_into_slip(&self, line: &str) -> Option<Slip> {
        let entries: Vec<&str> = line.split('\t').collect();

        let result = entries[0].parse::<u64>();

        if result.is_err() {
            error!("couldn't parse line : {:?}", line);
            error!("{:?}", result.err().unwrap());
            return None;
        }

        let amount = result.unwrap();

        // Check if amount is less than 25000 and set public key if so

        let publickey_str = if amount < 25000 {
            PROJECT_PUBLIC_KEY
        } else {
            entries[1]
        };

        let publickey_result = Self::decode_str(publickey_str);

        match publickey_result {
            Ok(val) => {
                let mut publickey_array: SaitoPublicKey = [0u8; 33];
                publickey_array.copy_from_slice(&val);

                // VipOutput is deprecated on mainnet
                let slip_type = match entries[2].trim_end_matches('\r') {
                    "VipOutput" => SlipType::Normal,
                    "Normal" => SlipType::Normal,
                    _ => panic!("Invalid slip type"),
                };

                let mut slip = Slip::default();
                slip.amount = amount;
                slip.public_key = publickey_array;
                slip.slip_type = slip_type;

                Some(slip)
            }
            Err(err) => {
                debug!("error reading issuance line {:?}", err);
                None
            }
        }
    }

    pub fn decode_str(string: &str) -> Result<Vec<u8>, bs58::decode::Error> {
        return bs58::decode(string).into_vec();
    }

    /// store the state of utxo balances given that map of balances and a treshold
    pub async fn write_utxoset_to_disk_path(
        &self,
        balance_map: AHashMap<SaitoPublicKey, u64>,
        threshold: u64,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("store to {}", path);
        let file_path = format!("{}", path);
        let mut file = File::create(&file_path)?;

        //assume normal txtype
        let txtype = "Normal";

        for (key, value) in &balance_map {
            if value > &threshold {
                let key_base58 = key.to_base58();
                let _ = writeln!(file, "{}\t{}\t{}", value, key_base58, txtype);
            }
        }
        debug!("written {} records", balance_map.len());
        Ok(())
    }

    /// store the state of utxo balances to standard file
    pub async fn write_utxoset_to_disk(
        &self,
        balance_map: AHashMap<SaitoPublicKey, u64>,
        threshold: u64,
    ) {
        let _ = self
            .write_utxoset_to_disk_path(balance_map, threshold, UTXOSTATE_FILE_PATH)
            .await;
    }
}

#[cfg(test)]
mod test {
    use log::{info, trace};

    use crate::core::consensus::block::Block;
    use crate::core::defs::{PrintForLog, SaitoHash};
    use crate::core::util::crypto::{hash, verify};
    use crate::core::util::test::test_manager::test::{create_timestamp, TestManager};

    // part is relative to it's cargo.toml

    // tests if issuance file can be read
    #[tokio::test]
    #[serial_test::serial]
    async fn read_issuance_file_test() {
        let t = TestManager::default();
        let read_result = t.storage.read(t.issuance_path).await;
        assert!(read_result.is_ok(), "Failed to read issuance file.");
    }

    // test if issuance file utxo is equal to the resultant balance map on created blockchain
    #[tokio::test]
    #[serial_test::serial]
    async fn issuance_hashmap_equals_balance_hashmap_test() {
        let mut t = TestManager::default();

        let issuance_hashmap = t.convert_issuance_to_hashmap(t.issuance_path).await;
        let slips = t
            .storage
            .get_token_supply_slips_from_disk_path(t.issuance_path)
            .await;

        t.initialize_from_slips(slips).await;
        let balance_map = t.balance_map().await;
        assert_eq!(issuance_hashmap, balance_map);
    }

    // // check if issuance occurs on block one
    // #[tokio::test]
    // async fn issuance_occurs_only_on_block_one_test() {
    //     let mut t = TestManager::new();
    //     let issuance_hashmap = t.convert_issuance_to_hashmap(TEST_ISSUANCE_FILEPATH).await;
    //     let slips = t
    //         .storage
    //         .get_token_supply_slips_from_disk_path(TEST_ISSUANCE_FILEPATH)
    //         .await;
    //     t.initialize_from_slips(slips).await;
    //     dbg!();

    //     assert_eq!(t.get_latest_block_id().await, 1);
    // }

    #[tokio::test]
    #[serial_test::serial]
    async fn write_read_block_to_file_test() {
        let mut t = TestManager::default();
        t.initialize(100, 100_000_000).await;

        let current_timestamp = create_timestamp();

        let mut block = Block::new();
        block.timestamp = current_timestamp;

        let filename = t.storage.write_block_to_disk(&mut block).await;
        trace!("block written to file : {}", filename);
        let retrieved_block = t.storage.load_block_from_disk(filename.as_str()).await;
        let mut actual_retrieved_block = retrieved_block.unwrap();
        actual_retrieved_block.generate();

        assert_eq!(block.timestamp, actual_retrieved_block.timestamp);
    }

    // TODO : delete this test
    #[ignore]
    #[tokio::test]
    async fn block_load_test_slr() {
        // pretty_env_logger::init();

        let t = TestManager::default();

        info!(
            "current dir = {:?}",
            std::env::current_dir().unwrap().to_str().unwrap()
        );
        let filename = std::env::current_dir().unwrap().to_str().unwrap().to_string() +
            "/data/blocks/1658821412997-f1bcf447a958018d38433adb6249c4cb4529af8f9613fdd8affd123d2a602dda.sai";
        let retrieved_block = t.storage.load_block_from_disk(filename.as_str()).await;
        let mut block = retrieved_block.unwrap();
        block.generate();

        info!(
            "prehash = {:?},  prev : {:?}",
            block.pre_hash.to_hex(),
            block.previous_block_hash.to_hex(),
        );

        assert_eq!(
            block.hash.to_hex(),
            "f1bcf447a958018d38433adb6249c4cb4529af8f9613fdd8affd123d2a602dda"
        );
        assert_ne!(block.timestamp, 0);

        let hex = hash(&block.pre_hash.to_vec());
        info!(
            "prehash = {:?}, hex = {:?}, signature : {:?}, creator = {:?}",
            block.pre_hash.to_hex(),
            hex.to_hex(),
            block.signature.to_hex(),
            block.creator.to_base58()
        );
        // assert_eq!("000000000000000a0000017d26dd628abcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bdcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bccccf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b000000000000000000000000000000000000000002faf08000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        //     hex::encode(block.serialize_for_signature()));
        let result = verify(
            &block.serialize_for_signature(),
            &block.signature,
            &block.creator,
        );
        assert!(result);

        let filename = t.storage.generate_block_filepath(&block);
        assert_eq!(
            filename,
            "./data/blocks/1658821412997-f1bcf447a958018d38433adb6249c4cb4529af8f9613fdd8affd123d2a602dda.sai"
        );
        // assert_eq!(retrieved_block.timestamp, 1637034582666);
    }

    #[test]
    fn hashing_test() {
        // pretty_env_logger::init();
        let h1: SaitoHash =
            hex::decode("fa761296cdca6b5c0e587e8bdc75f86223072780533a8edeb90fa51aea597128")
                .unwrap()
                .try_into()
                .unwrap();
        let h2: SaitoHash =
            hex::decode("8f1717d0f4a244f805436633897d48952c30cb35b3941e5d36cb371c68289d25")
                .unwrap()
                .try_into()
                .unwrap();
        let mut h3: Vec<u8> = vec![];
        h3.extend(&h1);
        h3.extend(&h2);

        let hash = hash(&h3);
        assert_eq!(
            hash.to_hex(),
            "de0cdde5db8fd4489f2038aca5224c18983f6676aebcb2561f5089e12ea2eedf"
        );
    }
}

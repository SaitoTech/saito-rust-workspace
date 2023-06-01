use std::sync::Arc;

use log::{debug, error, info, trace, warn};
use tokio::sync::RwLock;

use super::slip::SlipType;
use crate::common::defs::{push_lock, SaitoPublicKey, BLOCK_FILE_EXTENSION, LOCK_ORDER_MEMPOOL};
use crate::common::defs::{SaitoHash, MAX_TOKEN_SUPPLY};
use crate::common::interface_io::InterfaceIO;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::mempool::Mempool;
use crate::core::data::slip::Slip;
use crate::lock_for_write;
use bs58;

#[derive(Debug)]
pub struct Storage {
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
}

pub const ISSUANCE_FILE_PATH: &'static str = "./data/issuance/issuance";
pub const EARLYBIRDS_FILE_PATH: &'static str = "./data/issuance/earlybirds";
pub const DEFAULT_FILE_PATH: &'static str = "./data/issuance/default";

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
        let buffer = self.io_interface.read_value(path.to_string()).await;
        if buffer.is_err() {
            todo!()
        }
        let buffer = buffer.unwrap();
        Ok(buffer)
    }

    pub async fn write(&mut self, data: Vec<u8>, filename: &str) {
        self.io_interface
            .write_value(filename.to_string(), data)
            .await
            .expect("writing to storage failed");
    }

    pub async fn file_exists(&self, filename: &str) -> bool {
        self.io_interface
            .is_existing_file(filename.to_string())
            .await
    }

    pub fn generate_block_filepath(&self, block: &Block) -> String {
        self.io_interface.get_block_dir() + block.get_file_name().as_str()
    }
    pub async fn write_block_to_disk(&mut self, block: &Block) -> String {
        let buffer = block.serialize_for_net(BlockType::Full);
        let filename = self.generate_block_filepath(block);

        let result = self
            .io_interface
            .write_value(filename.clone(), buffer)
            .await;
        if result.is_err() {
            todo!()
        }
        filename
    }

    pub async fn load_blocks_from_disk(&mut self, mempool: Arc<RwLock<Mempool>>) {
        info!("loading blocks from disk");
        // self.get_token_supply_slips_from_disk().await;
        let file_names = self.io_interface.load_block_file_list().await;

        if file_names.is_err() {
            error!("failed loading blocks . {:?}", file_names.err().unwrap());
            return;
        }

        let mut file_names = file_names.unwrap();
        if file_names.is_empty() {
            // add slips
            println!("disk is empty, adding slip")
        }

        {
            file_names.sort();
            debug!("block file names : {:?}", file_names);

            trace!("loading files...");
            for file_name in file_names {
                info!("loading file : {:?}", file_name);
                let result = self
                    .io_interface
                    .read_value(self.io_interface.get_block_dir() + file_name.as_str())
                    .await;
                if result.is_err() {
                    todo!()
                }
                info!("file : {:?} loaded", file_name);
                let buffer: Vec<u8> = result.unwrap();
                let buffer_len = buffer.len();
                let result = Block::deserialize_from_net(buffer);
                if result.is_err() {
                    // ideally this shouldn't happen since we only write blocks which are valid to disk
                    warn!(
                        "failed deserializing block with buffer length : {:?}",
                        buffer_len
                    );
                    continue;
                }
                let mut block: Block = result.unwrap();
                block.force_loaded = true;
                block.generate();
                info!("block : {:?} loaded from disk", hex::encode(block.hash));
                let (mut mempool, _mempool_) = lock_for_write!(mempool, LOCK_ORDER_MEMPOOL);
                mempool.add_block(block);
            }
            trace!("block file loading finished");

            info!("loading blocks to mempool completed");
        }
    }

    pub async fn load_block_from_disk(&self, file_name: String) -> Result<Block, std::io::Error> {
        debug!("loading block {:?} from disk", file_name);
        let result = self.io_interface.read_value(file_name).await;
        if result.is_err() {
            todo!()
        }
        let buffer = result.unwrap();
        Block::deserialize_from_net(buffer)
    }

    pub async fn delete_block_from_disk(&self, filename: String) -> bool {
        self.io_interface.remove_value(filename).await.is_ok()
    }

    //
    // token issuance functions below
    //
    pub async fn get_token_supply_slips_from_disk(&self) -> Vec<Slip> {
        let mut v: Vec<Slip> = vec![];
        let mut tokens_issued = 0;
        //
        if self.file_exists(ISSUANCE_FILE_PATH).await {
            if let Ok(lines) = self
                .io_interface
                .read_value(ISSUANCE_FILE_PATH.to_string())
                .await
            {
                let mut contents = String::from_utf8(lines).expect("Failed to convert to String");
                contents = contents.trim_end_matches('\r').to_string();
                let lines: Vec<&str> = contents.split("\n").collect();

                for line in lines {
                    let line = line.trim_end_matches('\r');
                    if !line.is_empty() {
                        let slip = self.convert_issuance_into_slip(line);
                        v.push(slip);
                    }
                }

                for i in 0..v.len() {
                    tokens_issued += v[i].amount;
                }

                debug!("{:?} tokens issued", tokens_issued);
                return v;
            }
        } else {
            debug!("issuance file does not exist");
        }

        // if let Ok(lines) = Storage::read_lines_from_file(ISSUANCE_FILE_PATH) {
        //     for line in lines {
        //         if let Ok(ip) = line {
        //             let s = Storage::convert_issuance_into_slip(ip);
        //             v.push(s);
        //         }
        //     }
        // }
        // if let Ok(lines) = Storage::read_lines_from_file(EARLYBIRDS_FILE_PATH) {
        //     for line in lines {
        //         if let Ok(ip) = line {
        //             let s = Storage::convert_issuance_into_slip(ip);
        //             v.push(s);
        //         }
        //     }
        // }
        //

        //
        // if let Ok(lines) = Storage::read_lines_from_file(DEFAULT_FILE_PATH) {
        //     for line in lines {
        //         if let Ok(ip) = line {
        //             let mut s = Storage::convert_issuance_into_slip(ip);
        //             s.set_amount(MAX_TOKEN_SUPPLY - tokens_issued);
        //             v.push(s);
        //         }
        //     }
        // }

        return vec![];
    }

    fn convert_issuance_into_slip(&self, line: &str) -> Slip {
        let entries: Vec<&str> = line.split("\t").collect();
        debug!("{:?} here is an entry", &line);
        let amount = entries[0].parse::<u64>().expect("Failed to parse amount");
        debug!("{:?}here is an amount", &amount);
        let publickey_str = entries[1];
        let publickey_vec = bs58::decode(publickey_str)
            .into_vec()
            .expect("Decoding failed");
        let mut publickey_array: [u8; 33] = [0u8; 33];
        publickey_array.copy_from_slice(&publickey_vec);

        debug!(
            "{:?} {:?} {:?} here is an array",
            &entries[0], &entries[1], &entries[2]
        );

        // public_key_array.copy_from_slice(&v);
        // slip.public_key = public_key_array;
        let slip_type = match entries[2].trim_end_matches('\r') {
            "VipOutput" => SlipType::VipOutput,
            "Normal" => SlipType::Normal,
            _ => panic!("Invalid slip type"),
        };

        let mut slip = Slip {
            public_key: publickey_array,
            slip_index: 0,
            amount,
            block_id: 1,
            tx_ordinal: 0,
            slip_type,
            utxoset_key: [0; 58],
            is_utxoset_key_set: false,
        };

        debug!("{:?} here is a slip", slip);
        slip.generate_utxoset_key();
        println!("here is a slip: {:?}", slip);
        return slip;
    }
    // pub fn read_lines_from_file<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
    // where
    //     P: AsRef<Path>,
    // {
    //     // let file = File::open(filename)?;
    //     Ok(io::BufReader::new(file).lines())
    // }
    //
    // pub fn convert_issuance_into_slip(line: std::string::String) -> Slip {
    //     let mut iter = line.split_whitespace();
    //     let tmp = iter.next().unwrap();
    //     let tmp2 = iter.next().unwrap();
    //     let typ = iter.next().unwrap();

    //     let amt: u64 = tmp.parse::<u64>().unwrap();
    //     let tmp3 = tmp2.as_bytes();

    //     let mut add: SaitoPublicKey = [0; 33];
    //     for i in 0..33 {
    //         add[i] = tmp3[i];
    //     }

    //     let mut slip = Slip::default();
    //     slip.set_public_key(add);
    //     slip.set_amount(amt);
    //     if typ.eq("VipOutput") {
    //         slip.set_slip_type(SlipType::VipOutput);
    //     }
    //     if typ.eq("StakerDeposit") {
    //         slip.set_slip_type(SlipType::StakerDeposit);
    //     }
    //     if typ.eq("Normal") {
    //         slip.set_slip_type(SlipType::Normal);
    //     }

    //     return slip;
    // }
}

#[cfg(test)]
mod test {
    use log::{info, trace};

    use crate::common::defs::{SaitoHash, MAX_TOKEN_SUPPLY};
    use crate::common::test_manager::test::{create_timestamp, TestManager};
    use crate::core::data::block::Block;
    use crate::core::data::crypto::{hash, verify};

    #[ignore]
    #[tokio::test]
    async fn read_issuance_file_test() {
        let mut t = TestManager::new();
        t.initialize(100, 100_000_000).await;

        let slips = t.storage.get_token_supply_slips_from_disk();
        let mut total_issuance = 0;

        for i in 0..slips.len() {
            total_issuance += slips[i].amount;
        }

        assert_eq!(total_issuance, MAX_TOKEN_SUPPLY);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn write_read_block_to_file_test() {
        let mut t = TestManager::new();
        t.initialize(100, 100_000_000).await;

        let current_timestamp = create_timestamp();

        let mut block = Block::new();
        block.timestamp = current_timestamp;

        let filename = t.storage.write_block_to_disk(&mut block).await;
        trace!("block written to file : {}", filename);
        let retrieved_block = t.storage.load_block_from_disk(filename).await;
        let mut actual_retrieved_block = retrieved_block.unwrap();
        actual_retrieved_block.generate();

        assert_eq!(block.timestamp, actual_retrieved_block.timestamp);
    }

    // TODO : delete this test
    #[ignore]
    #[tokio::test]
    async fn block_load_test_slr() {
        // pretty_env_logger::init();

        let t = TestManager::new();

        info!(
            "current dir = {:?}",
            std::env::current_dir().unwrap().to_str().unwrap()
        );
        let filename = std::env::current_dir().unwrap().to_str().unwrap().to_string() +
            "/data/blocks/1658821412997-f1bcf447a958018d38433adb6249c4cb4529af8f9613fdd8affd123d2a602dda.sai";
        let retrieved_block = t.storage.load_block_from_disk(filename).await;
        let mut block = retrieved_block.unwrap();
        block.generate();

        info!(
            "prehash = {:?},  prev : {:?}",
            hex::encode(block.pre_hash),
            hex::encode(block.previous_block_hash),
        );
        // assert_eq!(
        //     "11bc1529b4bcdbfbdbd6582b9033e4156681e5b8777fcc5bcc0d69eb7238d133",
        //     hex::encode(block.pre_hash)
        // );
        // assert_eq!(
        //     "bcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b",
        //     hex::encode(block.previous_block_hash)
        // );
        assert_eq!(
            hex::encode(block.hash),
            "f1bcf447a958018d38433adb6249c4cb4529af8f9613fdd8affd123d2a602dda"
        );
        assert_ne!(block.timestamp, 0);

        let hex = hash(&block.pre_hash.to_vec());
        info!(
            "prehash = {:?}, hex = {:?}, signature : {:?}, creator = {:?}",
            hex::encode(block.pre_hash),
            hex::encode(hex),
            hex::encode(block.signature),
            hex::encode(block.creator)
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
            hex::encode(hash),
            "de0cdde5db8fd4489f2038aca5224c18983f6676aebcb2561f5089e12ea2eedf"
        );
    }
}

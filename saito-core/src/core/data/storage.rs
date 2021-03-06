use std::sync::Arc;

use crate::common::defs::BLOCK_FILE_EXTENSION;
use log::{debug, error, trace};
use tokio::sync::RwLock;

use crate::common::interface_io::InterfaceIO;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockchain::Blockchain;
use crate::core::data::network::Network;
use crate::core::data::slip::Slip;
use crate::core::mining_event_processor::MiningEvent;

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
        return self
            .io_interface
            .is_existing_file(filename.to_string())
            .await;
    }

    pub fn generate_block_filename(&self, block: &Block) -> String {
        let timestamp = block.timestamp;
        let block_hash = block.hash;
        self.io_interface.get_block_dir().to_string()
            + timestamp.to_string().as_str()
            + "-"
            + hex::encode(block_hash).as_str()
            + BLOCK_FILE_EXTENSION
    }
    pub async fn write_block_to_disk(&mut self, block: &Block) -> String {
        let buffer = block.serialize_for_net(BlockType::Full);
        let filename = self.generate_block_filename(block);

        let result = self
            .io_interface
            .write_value(filename.clone(), buffer)
            .await;
        if result.is_err() {
            todo!()
        }
        filename
    }

    pub async fn load_blocks_from_disk(
        &mut self,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        network: &Network,
        sender_to_miner: tokio::sync::mpsc::Sender<MiningEvent>,
    ) {
        debug!("loading blocks from disk");
        let file_names = self.io_interface.load_block_file_list().await;
        trace!("waiting for the blockchain write lock");
        let mut blockchain = blockchain_lock.write().await;
        trace!("acquired the blockchain write lock");

        if file_names.is_err() {
            error!("{:?}", file_names.err().unwrap());
            return;
        }
        let mut file_names = file_names.unwrap();
        file_names.sort();
        debug!("block file names : {:?}", file_names);
        for file_name in file_names {
            let result = self
                .io_interface
                .read_value(self.io_interface.get_block_dir() + file_name.as_str())
                .await;
            if result.is_err() {
                todo!()
            }
            let buffer = result.unwrap();
            let mut block = Block::deserialize_from_net(&buffer);
            block.generate();
            debug!("adding block from file : {:?}", file_name);
            blockchain
                .add_block(block, network, self, sender_to_miner.clone())
                .await;
        }
    }

    pub async fn load_block_from_disk(&self, file_name: String) -> Result<Block, std::io::Error> {
        debug!("loading block {:?} from disk", file_name);
        let result = self.io_interface.read_value(file_name).await;
        if result.is_err() {
            todo!()
        }
        let buffer = result.unwrap();
        Ok(Block::deserialize_from_net(&buffer))
    }

    pub async fn delete_block_from_disk(&self, filename: String) -> bool {
        self.io_interface.remove_value(filename).await.is_ok()
    }

    //
    // token issuance functions below
    //
    pub fn return_token_supply_slips_from_disk(&self) -> Vec<Slip> {
        // let mut v: Vec<Slip> = vec![];
        // let mut tokens_issued = 0;
        //
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
        // for i in 0..v.len() {
        //     tokens_issued += v[i].get_amount();
        // }
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
    //
    //     let amt: u64 = tmp.parse::<u64>().unwrap();
    //     let tmp3 = tmp2.as_bytes();
    //
    //     let mut add: SaitoPublicKey = [0; 33];
    //     for i in 0..33 {
    //         add[i] = tmp3[i];
    //     }
    //
    //     let mut slip = Slip::new();
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
    //
    //     return slip;
    // }
}

#[cfg(test)]
mod test {
    use crate::common::defs::SaitoHash;
    use crate::common::test_manager::test::{create_timestamp, TestManager};
    use crate::core::data::block::Block;
    use crate::core::data::blockchain::MAX_TOKEN_SUPPLY;
    use crate::core::data::crypto::{hash, verify};
    use log::info;

    #[ignore]
    #[tokio::test]
    async fn read_issuance_file_test() {
        let mut t = TestManager::new();
        t.initialize(100, 100_000_000).await;

        let slips = t.storage.return_token_supply_slips_from_disk();
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
        t.initialize(100, 100_000_000);

        let current_timestamp = create_timestamp();

        let mut block = Block::new();
        block.timestamp = current_timestamp;

        let filename = t.storage.write_block_to_disk(&mut block).await;
        log::trace!("block written to file : {}", filename);
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
        let filename =std::env::current_dir().unwrap().to_str().unwrap().to_string()+
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
            block.signature,
            block.creator,
        );
        assert!(result);

        let filename = t.storage.generate_block_filename(&block);
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

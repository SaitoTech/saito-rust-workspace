use std::sync::Arc;

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
            + ".block"
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
        let file_names = file_names.unwrap();
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
    use crate::common::test_manager::test::{create_timestamp, TestManager};
    use crate::core::data::block::Block;
    use crate::core::data::blockchain::MAX_TOKEN_SUPPLY;

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
}

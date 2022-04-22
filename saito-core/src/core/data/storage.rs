use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::debug;
use tokio::sync::RwLock;

use crate::common::handle_io::HandleIo;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockchain::Blockchain;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::slip::Slip;
use crate::core::miner_controller::MinerEvent;

pub struct Storage {
    io_handler: dyn HandleIo,
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
    /// read from a path to a Vec<u8>
    pub async fn read(
        path: &str,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> std::io::Result<Vec<u8>> {
        let buffer = io_handler.read_value(path.to_string()).await;
        if buffer.is_err() {
            todo!()
        }
        let buffer = buffer.unwrap();
        Ok(buffer)
    }

    pub async fn write(
        data: Vec<u8>,
        filename: &str,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        io_handler
            .write_value(filename.to_string(), data)
            .await
            .expect("writing to storage failed");
    }

    pub async fn file_exists(
        filename: &str,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> bool {
        return io_handler.is_existing_file(filename.to_string()).await;
    }

    pub fn generate_block_filename(block: &Block) -> String {
        let timestamp = block.get_timestamp();
        let block_hash = block.get_hash();
        "data/".to_string()
            + timestamp.to_string().as_str()
            + "-"
            + hex::encode(block_hash).as_str()
            + ".block"
    }
    pub async fn write_block_to_disk(
        block: &Block,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> String {
        let buffer = block.serialize_for_net(BlockType::Full);
        let filename = Storage::generate_block_filename(block);

        let result = io_handler.write_value(filename.clone(), buffer).await;
        filename
    }

    pub async fn load_blocks_from_disk(
        blockchain_lock: Arc<RwLock<Blockchain>>,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_miner: tokio::sync::mpsc::Sender<MinerEvent>,
    ) {
        debug!("loading blocks from disk");
        let file_names = io_handler.load_block_file_list().await;
        let mut blockchain = blockchain_lock.write().await;

        let file_names = file_names.unwrap();
        for file_name in file_names {
            let result = io_handler.read_value(file_name).await;
            if result.is_err() {
                todo!()
            }
            let buffer = result.unwrap();
            let mut block = Block::deserialize_for_net(&buffer);
            block.generate_metadata();
            blockchain
                .add_block(block, io_handler, peers.clone(), sender_to_miner.clone())
                .await;
        }
    }

    pub async fn load_block_from_disk(
        file_name: String,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> Result<Block, std::io::Error> {
        debug!("loading block {:?} from disk", file_name);
        let result = io_handler.read_value(file_name).await;
        if result.is_err() {
            todo!()
        }
        let buffer = result.unwrap();
        Ok(Block::deserialize_for_net(&buffer))
    }

    pub async fn delete_block_from_disk(
        filename: String,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> bool {
        // TODO: get rid of this function or make it useful.
        // it should match the result and provide some error handling.
        // let _res = std::fs::remove_file(filename);
        true
    }

    //
    // token issuance functions below
    //
    pub fn return_token_supply_slips_from_disk() -> Vec<Slip> {
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
    //     slip.set_publickey(add);
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

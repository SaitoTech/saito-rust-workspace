use std::io::{Error, ErrorKind, Read};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::handle_io::HandleIo;
use crate::core::blockchain::Blockchain;
use crate::core::data::block::Block;
use crate::core::data::slip::Slip;

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
    pub fn read(path: &str) -> std::io::Result<Vec<u8>> {
        Ok(vec![])
    }

    pub fn write(data: Vec<u8>, filename: &str) {}

    pub fn file_exists(filename: &str) -> bool {
        false
    }

    pub fn generate_block_filename(block: &Block) -> String {
        "".parse().unwrap()
    }
    pub fn write_block_to_disk(block: &mut Block) -> String {
        "".parse().unwrap()
    }

    pub async fn load_blocks_from_disk(blockchain_lock: Arc<RwLock<Blockchain>>) {}

    pub async fn load_block_from_disk(filename: String) -> Result<Block, std::io::Error> {
        // let file_to_load = &filename;
        // let mut f = File::open(file_to_load).unwrap();
        // let mut encoded = Vec::<u8>::new();
        // f.read_to_end(&mut encoded).unwrap();
        // Block::deserialize_for_net(&vec![])
        Err(Error::from(ErrorKind::NotFound))
    }

    pub async fn delete_block_from_disk(filename: String) -> bool {
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

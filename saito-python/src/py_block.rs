use num_traits::FromPrimitive;
use pyo3::pyclass;

use saito_core::core::consensus::block::{Block, BlockType};
use saito_core::core::consensus::transaction::Transaction;
use saito_core::core::defs::{Currency, PrintForLog, Timestamp};

use crate::py_transaction::WasmTransaction;
use crate::saitopython::{string_to_hex, string_to_key};

#[pyclass]
pub struct WasmBlock {
    block: Block,
}

impl WasmBlock {
    pub fn new() -> WasmBlock {
        WasmBlock {
            block: Block::new(),
        }
    }
    pub fn get_transactions(&self) -> Vec<WasmTransaction> {
        let txs: Vec<WasmTransaction> = self
            .block
            .transactions
            .iter()
            .map(|tx| WasmTransaction::from_transaction(tx.clone()))
            .collect();

        txs
    }

    pub fn fee_transaction(&self) -> WasmTransaction {
        if let Some(tx) = &self.block.cv.fee_transaction {
            let tx = WasmTransaction::from_transaction(tx.clone());
            tx
        } else {
            let tx = WasmTransaction {
                tx: Transaction::default(),
            };
            tx
        }
    }

    pub fn it_num(&self) -> u8 {
        self.block.cv.it_num
    }

    pub fn it_index(&self) -> u32 {
        if let Some(it_index) = self.block.cv.it_index {
            it_index as u32
        } else {
            0
        }
    }

    pub fn avg_fee_per_byte(&self) -> u64 {
        self.block.cv.avg_fee_per_byte
    }

    pub fn burnfee(&self) -> u64 {
        self.block.cv.burnfee
    }

    pub fn ft_num(&self) -> u8 {
        self.block.cv.ft_num
    }

    pub fn ft_index(&self) -> u32 {
        if let Some(ft_index) = self.block.cv.ft_index {
            ft_index as u32
        } else {
            0
        }
    }

    pub fn gt_num(&self) -> u8 {
        self.block.cv.gt_num
    }

    pub fn gt_index(&self) -> u32 {
        if let Some(gt_index) = self.block.cv.gt_index {
            gt_index as u32
        } else {
            0
        }
    }

    pub fn total_fees(&self) -> u64 {
        self.block.cv.total_fees
    }

    pub fn difficulty(&self) -> u64 {
        self.block.cv.difficulty
    }

    // pub fn rebroadcasts(&self) -> Array {
    //     let mut txs: Vec<WasmTransaction> = self
    //         .block
    //         .cv
    //         .rebroadcasts
    //         .iter()
    //         .map(|tx| WasmTransaction::from_transaction(tx.clone()))
    //         .collect();
    //     let array = js_sys::Array::new_with_length(txs.len() as u32);
    //     for (i, tx) in txs.drain(..).enumerate() {
    //         array.set(i as u32, JsValue::from(tx));
    //     }
    //     array
    // }

    pub fn total_rebroadcast_slips(&self) -> u64 {
        self.block.cv.total_rebroadcast_slips
    }

    pub fn total_rebroadcast_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_nolan
    }

    pub fn total_rebroadcast_fees_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_fees_nolan
    }

    pub fn total_rebroadcast_staking_payouts_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_staking_payouts_nolan
    }

    pub fn avg_nolan_rebroadcast_per_block(&self) -> u64 {
        self.block.cv.avg_nolan_rebroadcast_per_block
    }

    pub fn rebroadcast_hash(&self) -> String {
        // Convert the byte array to a JsValue
        // JsValue::from_serde(&self.block.cv.rebroadcast_hash).unwrap()
        self.block.cv.rebroadcast_hash.to_hex().into()
    }

    // TODO -- deprecated
    pub fn avg_income(&self) -> u64 {
        self.block.cv.avg_total_fees
    }

    pub fn avg_total_fees(&self) -> u64 {
        self.block.cv.avg_total_fees
    }

    pub fn get_id(&self) -> u64 {
        self.block.id
    }
    pub fn set_id(&mut self, id: u64) {
        self.block.id = id;
    }
    pub fn get_timestamp(&self) -> Timestamp {
        self.block.timestamp
    }
    pub fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.block.timestamp = timestamp;
    }
    pub fn get_previous_block_hash(&self) -> String {
        self.block.previous_block_hash.to_hex().into()
    }
    pub fn set_previous_block_hash(&mut self, hash: String) {
        self.block.previous_block_hash = string_to_hex(hash).unwrap();
    }
    pub fn set_creator(&mut self, key: String) {
        self.block.creator = string_to_key(key).unwrap();
    }
    pub fn get_creator(&self) -> String {
        self.block.creator.to_base58().into()
    }
    pub fn get_type(&self) -> u8 {
        self.block.block_type as u8
    }
    pub fn set_type(&mut self, t: u8) {
        self.block.block_type = BlockType::from_u8(t).unwrap();
    }
    pub fn get_hash(&self) -> String {
        self.block.hash.to_hex().into()
    }

    pub fn in_longest_chain(&self) -> bool {
        self.block.in_longest_chain
    }

    pub fn force_loaded(&self) -> bool {
        self.block.force_loaded
    }

    // pub fn get_cv(&self) -> JsValue {
    //     let cv = &self.block.cv;
    //     JsValue::from(WasmConsensusValues::from_cv(cv.clone()))
    // }

    pub fn get_file_name(&self) -> String {
        self.block.get_file_name().into()
    }

    // pub fn serialize(&self) -> Uint8Array {
    //     let buffer = self.block.serialize_for_net(BlockType::Full);
    //     let buf = Uint8Array::new_with_length(buffer.len() as u32);
    //     buf.copy_from(buffer.as_slice());
    //     buf
    // }
    // pub fn deserialize(&mut self, buffer: Uint8Array) -> Result<JsValue, JsValue> {
    //     let buffer = buffer.to_vec();
    //     let mut block = Block::deserialize_from_net(&buffer).or(Err(JsValue::from("failed")))?;
    //
    //     block.generate();
    //     self.block = block;
    //
    //     Ok(JsValue::from(""))
    // }
    // pub fn has_keylist_txs(&self, keylist: Array) -> bool {
    //     let keylist = Self::convert_keylist(keylist);
    //     return self.block.has_keylist_txs(keylist);
    // }
    // pub fn generate_lite_block(&self, keylist: Array) -> WasmBlock {
    //     let keylist = Self::convert_keylist(keylist);
    //     let block = self.block.generate_lite_block(keylist);
    //     WasmBlock::from_block(block)
    // }

    pub fn treasury(&self) -> Currency {
        self.block.treasury
    }
    pub fn graveyard(&self) -> Currency {
        self.block.graveyard
    }
}

impl WasmBlock {
    pub fn from_block(block: Block) -> WasmBlock {
        WasmBlock { block }
    }

    // pub fn convert_keylist(keylist: Array) -> Vec<SaitoPublicKey> {
    //     let mut keys = vec![];
    //     for i in 0..(keylist.length() as u32) {
    //         // TODO : check data types/lengths before this to avoid attacks
    //         let key = keylist.get(i).as_string();
    //         if key.is_none() {
    //             // error!("couldn't convert value : {:?} to string", keylist.get(i));
    //             return vec![];
    //             // continue;
    //         }
    //         let key = key.unwrap();
    //         let key = SaitoPublicKey::from_base58(key.as_str());
    //
    //         if key.is_err() {
    //             error!("key : {:?}", keylist.get(i));
    //             error!("error decoding key list : {:?}", key.err().unwrap());
    //             return vec![];
    //         }
    //         keys.push(key.unwrap());
    //     }
    //     keys
    // }
}

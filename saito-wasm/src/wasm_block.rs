use js_sys::{Array, JsString, Uint8Array};
use log::{error, info};
use num_traits::FromPrimitive;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::{PrintForLog, SaitoPublicKey, Timestamp};
use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::transaction::Transaction;

use crate::saitowasm::{string_to_hex, string_to_key};
use crate::wasm_consensus_values::{WasmBlockPayout, WasmConsensusValues};
use crate::wasm_transaction::WasmTransaction;

#[wasm_bindgen]
pub struct WasmBlock {
    block: Block,
}

#[wasm_bindgen]
impl WasmBlock {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBlock {
        WasmBlock {
            block: Block::new(),
        }
    }
    #[wasm_bindgen(getter = transactions)]
    pub fn get_transactions(&self) -> Array {
        let mut txs: Vec<WasmTransaction> = self
            .block
            .transactions
            .iter()
            .map(|tx| WasmTransaction::from_transaction(tx.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(txs.len() as u32);
        for (i, tx) in txs.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(tx));
        }
        array
    }

    // #[wasm_bindgen(getter= fee_transaction)]
    pub fn fee_transaction(&self) -> JsValue {
        if let Some(tx) = &self.block.cv.fee_transaction {
            let tx = WasmTransaction::from_transaction(tx.clone());
            JsValue::from(tx)
        } else {
            let tx = WasmTransaction {
                tx: Transaction::default(),
            };
            JsValue::from(tx)
        }
    }

    pub fn it_num(&self) -> u8 {
        self.block.cv.it_num
    }

    #[wasm_bindgen(getter = it_index)]
    pub fn it_index(&self) -> u32 {
        if let Some(it_index) = self.block.cv.it_index {
            it_index as u32
        } else {
            0
        }
    }

    #[wasm_bindgen(getter = block_payout)]
    pub fn get_block_payout(&self) -> Array {
        let mut block_payout: Vec<WasmBlockPayout> = self
            .block
            .cv
            .block_payout
            .iter()
            .map(|bp| WasmBlockPayout::from_block_payout(bp.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(block_payout.len() as u32);

        for (i, bpo) in block_payout.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(bpo));
        }
        array
    }

    #[wasm_bindgen(getter = ft_num)]
    pub fn ft_num(&self) -> u8 {
        self.block.cv.ft_num
    }

    #[wasm_bindgen(getter = ft_index)]
    pub fn ft_index(&self) -> u32 {
        if let Some(ft_index) = self.block.cv.ft_index {
            ft_index as u32
        } else {
            0
        }
    }

    // #[wasm_bindgen(getter = gt_num)]
    // pub fn gt_num(&self) -> u8 {
    //     self.block.cv.gt_num
    // }

    #[wasm_bindgen(getter = gt_index)]
    pub fn gt_index(&self) -> u32 {
        if let Some(gt_index) = self.block.cv.gt_index {
            gt_index as u32
        } else {
            0
        }
    }

    #[wasm_bindgen(getter = total_fees)]
    pub fn total_fees(&self) -> u64 {
        self.block.cv.total_fees
    }

    #[wasm_bindgen(getter = expected_difficulty)]
    pub fn expected_difficulty(&self) -> u64 {
        self.block.cv.expected_difficulty
    }

    #[wasm_bindgen(getter = avg_atr_income)]
    pub fn avg_atr_income(&self) -> u64 {
        self.block.cv.avg_atr_income
    }

    #[wasm_bindgen(getter = avg_atr_variance)]
    pub fn avg_atr_variance(&self) -> u64 {
        self.block.cv.avg_atr_variance
    }

    #[wasm_bindgen(getter = rebroadcasts)]
    pub fn rebroadcasts(&self) -> Array {
        let mut txs: Vec<WasmTransaction> = self
            .block
            .cv
            .rebroadcasts
            .iter()
            .map(|tx| WasmTransaction::from_transaction(tx.clone()))
            .collect();
        let array = js_sys::Array::new_with_length(txs.len() as u32);
        for (i, tx) in txs.drain(..).enumerate() {
            array.set(i as u32, JsValue::from(tx));
        }
        array
    }

    #[wasm_bindgen(getter = total_rebroadcast_slips)]
    pub fn total_rebroadcast_slips(&self) -> u64 {
        self.block.cv.total_rebroadcast_slips
    }

    #[wasm_bindgen(getter = total_rebroadcast_nolan)]
    pub fn total_rebroadcast_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_nolan
    }

    #[wasm_bindgen(getter = total_rebroadcast_fees_nolan)]
    pub fn total_rebroadcast_fees_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_fees_nolan
    }

    #[wasm_bindgen(getter = total_rebroadcast_staking_payouts_nolan)]
    pub fn total_rebroadcast_staking_payouts_nolan(&self) -> u64 {
        self.block.cv.total_rebroadcast_staking_payouts_nolan
    }

    #[wasm_bindgen(getter = rebroadcast_hash)]
    pub fn rebroadcast_hash(&self) -> JsString {
        // Convert the byte array to a JsValue
        // JsValue::from_serde(&self.block.cv.rebroadcast_hash).unwrap()
        self.block.cv.rebroadcast_hash.to_hex().into()
    }

    #[wasm_bindgen(getter = nolan_falling_off_chain)]
    pub fn nolan_falling_off_chain(&self) -> u64 {
        self.block.cv.nolan_falling_off_chain
    }

    #[wasm_bindgen(getter = staking_treasury)]
    pub fn staking_treasury(&self) -> u64 {
        self.block.cv.staking_treasury
    }

    #[wasm_bindgen(getter = avg_income)]
    pub fn avg_income(&self) -> u64 {
        self.block.cv.avg_income
    }

    #[wasm_bindgen(getter = avg_variance)]
    pub fn avg_variance(&self) -> u64 {
        self.block.cv.avg_variance
    }

    #[wasm_bindgen(getter = id)]
    pub fn get_id(&self) -> u64 {
        self.block.id
    }
    #[wasm_bindgen(setter = id)]
    pub fn set_id(&mut self, id: u64) {
        self.block.id = id;
    }
    #[wasm_bindgen(getter = timestamp)]
    pub fn get_timestamp(&self) -> Timestamp {
        self.block.timestamp
    }
    #[wasm_bindgen(setter = timestamp)]
    pub fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.block.timestamp = timestamp;
    }
    #[wasm_bindgen(getter = previous_block_hash)]
    pub fn get_previous_block_hash(&self) -> JsString {
        self.block.previous_block_hash.to_hex().into()
    }
    #[wasm_bindgen(setter = previous_block_hash)]
    pub fn set_previous_block_hash(&mut self, hash: JsString) {
        self.block.previous_block_hash = string_to_hex(hash).unwrap();
    }
    #[wasm_bindgen(setter = creator)]
    pub fn set_creator(&mut self, key: JsString) {
        self.block.creator = string_to_key(key).unwrap();
    }
    #[wasm_bindgen(getter = creator)]
    pub fn get_creator(&self) -> JsString {
        self.block.creator.to_base58().into()
    }
    #[wasm_bindgen(getter = type)]
    pub fn get_type(&self) -> u8 {
        self.block.block_type as u8
    }
    #[wasm_bindgen(setter = type)]
    pub fn set_type(&mut self, t: u8) {
        self.block.block_type = BlockType::from_u8(t).unwrap();
    }
    #[wasm_bindgen(getter = hash)]
    pub fn get_hash(&self) -> JsString {
        self.block.hash.to_hex().into()
    }

    #[wasm_bindgen(getter = in_longest_chain)]
    pub fn in_longest_chain(&self) -> bool {
        self.block.in_longest_chain
    }

    #[wasm_bindgen(getter = force_loaded)]
    pub fn force_loaded(&self) -> bool {
        self.block.force_loaded
    }

    #[wasm_bindgen(getter = cv)]
    pub fn get_cv(&self) -> JsValue {
        let cv = &self.block.cv;
        JsValue::from(WasmConsensusValues::from_cv(cv.clone()))
    }

    #[wasm_bindgen(getter = file_name)]
    pub fn get_file_name(&self) -> JsString {
        self.block.get_file_name().into()
    }

    pub fn serialize(&self) -> Uint8Array {
        let buffer = self.block.serialize_for_net(BlockType::Full);
        let buf = Uint8Array::new_with_length(buffer.len() as u32);
        buf.copy_from(buffer.as_slice());
        buf
    }
    pub fn deserialize(&mut self, buffer: Uint8Array) -> Result<JsValue, JsValue> {
        let buffer = buffer.to_vec();
        let mut block = Block::deserialize_from_net(buffer).unwrap();

        block.generate();
        self.block = block;

        Ok(JsValue::from(""))
    }
    pub fn has_keylist_txs(&self, keylist: Array) -> bool {
        let keylist = Self::convert_keylist(keylist);
        return self.block.has_keylist_txs(keylist);
    }
    pub fn generate_lite_block(&self, keylist: Array) -> WasmBlock {
        let keylist = Self::convert_keylist(keylist);
        let block = self.block.generate_lite_block(keylist);
        WasmBlock::from_block(block)
    }
}

impl WasmBlock {
    pub fn from_block(block: Block) -> WasmBlock {
        WasmBlock { block }
    }

    pub fn convert_keylist(keylist: Array) -> Vec<SaitoPublicKey> {
        let mut keys = vec![];
        for i in 0..(keylist.length() as u32) {
            // TODO : check data types/lengths before this to avoid attacks
            let key = keylist.get(i).as_string();
            if key.is_none() {
                // error!("couldn't convert value : {:?} to string", keylist.get(i));
                return vec![];
                // continue;
            }
            let key = key.unwrap();
            let key = SaitoPublicKey::from_base58(key.as_str());

            if key.is_err() {
                error!("key : {:?}", keylist.get(i));
                error!("error decoding key list : {:?}", key.err().unwrap());
                return vec![];
            }
            keys.push(key.unwrap());
        }
        keys
    }
}

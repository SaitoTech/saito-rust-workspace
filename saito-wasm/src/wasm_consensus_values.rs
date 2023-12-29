use js_sys::{Array, JsString};
use saito_core::common::defs::{Currency, PrintForLog};
use saito_core::core::data::block::{BlockPayout, ConsensusValues};
use saito_core::core::data::transaction::Transaction;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use crate::wasm_transaction::WasmTransaction;

#[wasm_bindgen]
#[derive(Clone)]
pub struct WasmConsensusValues {
    pub(crate) cv: ConsensusValues,
}

#[wasm_bindgen]
impl WasmConsensusValues {
    #[wasm_bindgen(getter = it_num)]
    pub fn it_num(&self) -> u8 {
        self.cv.it_num
    }

    // #[wasm_bindgen(getter= fee_transaction)]
    pub fn fee_transaction(&self) -> JsValue {
        if let Some(tx) = &self.cv.fee_transaction {
            let tx = WasmTransaction::from_transaction(tx.clone());
            JsValue::from(tx)
        } else {
            let tx = WasmTransaction {
                tx: Transaction::default(),
            };
            JsValue::from(tx)
        }
    }

    #[wasm_bindgen(getter = it_index)]
    pub fn it_index(&self) -> u32 {
        if let Some(it_index) = self.cv.it_index {
            it_index as u32
        } else {
            0
        }
    }

    #[wasm_bindgen(getter = block_payout)]
    pub fn get_block_payout(&self) -> Array {
        let mut block_payout: Vec<WasmBlockPayout> = self
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
        self.cv.ft_num
    }

    #[wasm_bindgen(getter = ft_index)]
    pub fn ft_index(&self) -> u32 {
        if let Some(ft_index) = self.cv.ft_index {
            ft_index as u32
        } else {
            0
        }
    }

    #[wasm_bindgen(getter = gt_index)]
    pub fn gt_index(&self) -> u32 {
        if let Some(gt_index) = self.cv.gt_index {
            gt_index as u32
        } else {
            0
        }
    }

    #[wasm_bindgen(getter = total_fees)]
    pub fn total_fees(&self) -> u64 {
        self.cv.total_fees
    }

    #[wasm_bindgen(getter = expected_difficulty)]
    pub fn expected_difficulty(&self) -> u64 {
        self.cv.expected_difficulty
    }

    #[wasm_bindgen(getter = total_rebroadcast_slips)]
    pub fn total_rebroadcast_slips(&self) -> u64 {
        self.cv.total_rebroadcast_slips
    }

    #[wasm_bindgen(getter = total_rebroadcast_nolan)]
    pub fn total_rebroadcast_nolan(&self) -> u64 {
        self.cv.total_rebroadcast_nolan
    }

    #[wasm_bindgen(getter = total_rebroadcast_fees_nolan)]
    pub fn total_rebroadcast_fees_nolan(&self) -> u64 {
        self.cv.total_rebroadcast_fees_nolan
    }

    #[wasm_bindgen(getter = total_rebroadcast_staking_payouts_nolan)]
    pub fn total_rebroadcast_staking_payouts_nolan(&self) -> u64 {
        self.cv.total_rebroadcast_staking_payouts_nolan
    }

    #[wasm_bindgen(getter = rebroadcast_hash)]
    pub fn rebroadcast_hash(&self) -> JsString {
        // Convert the byte array to a JsValue
        // JsValue::from_serde(&self.cv.rebroadcast_hash).unwrap()
        self.cv.rebroadcast_hash.to_hex().into()
    }

    #[wasm_bindgen(getter = nolan_falling_off_chain)]
    pub fn nolan_falling_off_chain(&self) -> u64 {
        self.cv.nolan_falling_off_chain
    }

    #[wasm_bindgen(getter = staking_treasury)]
    pub fn staking_treasury(&self) -> u64 {
        self.cv.staking_treasury
    }

    // #[wasm_bindgen(getter = block_payout)]
    // pub fn block_payout(&self) -> JsValue {
    //     // assuming you can convert Vec<BlockPayout> to a JsValue
    //     // this might require a method or utility to convert properly
    //     JsValue::from_serde(&self.cv.block_payout).unwrap()
    // }

    #[wasm_bindgen(getter = avg_income)]
    pub fn avg_income(&self) -> u64 {
        self.cv.avg_income
    }

    #[wasm_bindgen(getter = avg_variance)]
    pub fn avg_variance(&self) -> u64 {
        self.cv.avg_variance
    }
}

impl WasmConsensusValues {
    pub fn from_cv(cv: ConsensusValues) -> WasmConsensusValues {
        WasmConsensusValues { cv }
    }
}

#[wasm_bindgen]
pub struct WasmBlockPayout {
    pub(crate) block_payout: BlockPayout,
}

#[wasm_bindgen]
impl WasmBlockPayout {
    #[wasm_bindgen(getter = miner)]
    pub fn miner(&self) -> JsString {
        self.block_payout.miner.to_base58().into()
    }

    #[wasm_bindgen(getter = router)]
    pub fn router(&self) -> JsString {
        self.block_payout.router.to_base58().into()
    }

    #[wasm_bindgen(getter = miner_payout)]
    pub fn miner_payout(&self) -> Currency {
        self.block_payout.miner_payout
    }

    #[wasm_bindgen(getter = router_payout)]
    pub fn router_payout(&self) -> Currency {
        self.block_payout.router_payout
    }

    #[wasm_bindgen(getter = staking_treasury)]
    pub fn staking_treasury(&self) -> i64 {
        self.block_payout.staking_treasury
    }

    #[wasm_bindgen(getter = random_number)]
    pub fn random_number(&self) -> JsString {
        self.block_payout.random_number.to_hex().into()
    }
}

impl WasmBlockPayout {
    pub fn from_block_payout(block_payout: BlockPayout) -> WasmBlockPayout {
        WasmBlockPayout { block_payout }
    }
}

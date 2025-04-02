use js_sys::{Array, JsString, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;

use saito_core::core::consensus::wallet::NFT;

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct WasmNFT {
    pub(crate) nft: NFT,
}

#[wasm_bindgen]
impl WasmNFT {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNFT {
        WasmNFT {
            nft: NFT::default(),
        }
    }

    #[wasm_bindgen(getter = utxokey_bound)]
    pub fn get_utxokey_bound(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.utxokey_bound.len() as u32);
        buffer.copy_from(self.nft.utxokey_bound.as_slice());
        buffer
    }

    #[wasm_bindgen(getter = utxokey_normal)]
    pub fn get_utxokey_normal(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.utxokey_normal.len() as u32);
        buffer.copy_from(self.nft.utxokey_normal.as_slice());
        buffer
    }

    #[wasm_bindgen(getter = nft_id)]
    pub fn get_nft_id(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.nft_id.len() as u32);
        buffer.copy_from(self.nft.nft_id.as_slice());
        buffer
    }

    #[wasm_bindgen(getter = tx_sig)]
    pub fn get_tx_sig(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.tx_sig.len() as u32);
        buffer.copy_from(self.nft.tx_sig.as_slice());
        buffer
    }
}

impl WasmNFT {
    pub fn from_nft(nft: NFT) -> WasmNFT {
        WasmNFT { nft }
    }
}

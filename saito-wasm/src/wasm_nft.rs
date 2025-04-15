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

    #[wasm_bindgen(getter = nft_slip1_utxokey)]
    pub fn get_nft_slip1_utxokey(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.nft_slip1_utxokey.len() as u32);
        buffer.copy_from(self.nft.nft_slip1_utxokey.as_slice());
        buffer
    }

    #[wasm_bindgen(getter = normal_slip_utxokey)]
    pub fn get_normal_slip_utxokey(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.normal_slip_utxokey.len() as u32);
        buffer.copy_from(self.nft.normal_slip_utxokey.as_slice());
        buffer
    }

    #[wasm_bindgen(getter = nft_slip2_utxokey)]
    pub fn get_nft_slip2_utxokey(&self) -> Uint8Array {
        let buffer = Uint8Array::new_with_length(self.nft.nft_slip2_utxokey.len() as u32);
        buffer.copy_from(self.nft.nft_slip2_utxokey.as_slice());
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

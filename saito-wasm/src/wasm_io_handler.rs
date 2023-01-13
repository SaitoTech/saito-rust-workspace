use std::fmt::{Debug, Formatter};
use std::io::Error;

use async_trait::async_trait;
use js_sys::{Array, BigInt, Uint8Array};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::SaitoHash;
use saito_core::common::interface_io::InterfaceIO;
use saito_core::core::data::block::Block;
use saito_core::core::data::configuration::PeerConfig;

pub struct WasmIoHandler {}

#[async_trait]
impl InterfaceIO for WasmIoHandler {
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        let array = js_sys::Uint8Array::new_with_length(buffer.len() as u32);
        array.copy_from(buffer.as_slice());

        MsgHandler::send_message(js_sys::BigInt::from(peer_index), array);
        Ok(())
    }

    async fn send_message_to_all(
        &self,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn connect_to_peer(&mut self, peer: PeerConfig) -> Result<(), Error> {
        todo!()
    }

    // async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error> {
    //     todo!()
    // }

    async fn disconnect_from_peer(&mut self, peer_index: u64) -> Result<(), Error> {
        todo!()
    }

    // fn set_write_result(
    //     &mut self,
    //     result_key: String,
    //     result: Result<String, Error>,
    // ) -> Result<(), Error> {
    //     todo!()
    // }

    async fn fetch_block_from_peer(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }

    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        todo!()
    }

    async fn is_existing_file(&self, key: String) -> bool {
        todo!()
    }

    async fn remove_value(&self, key: String) -> Result<(), Error> {
        todo!()
    }

    fn get_block_dir(&self) -> String {
        "data/blocks/".to_string()
    }
}

impl Debug for WasmIoHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustIoHandler")
            // .field("handler_id", &self.handler_id)
            .finish()
    }
}

#[wasm_bindgen(module = "/js/msg_handler.js")]
extern "C" {
    type MsgHandler;

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn send_message(peer_index: js_sys::BigInt, buffer: js_sys::Uint8Array);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn send_message_to_all(buffer: js_sys::Uint8Array, exceptions: js_sys::Array);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn connect_to_peer(url: String) -> BigInt;
    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn write_value(key: String, value: Uint8Array);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn read_value(key: String) -> JsValue;

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn load_block_file_list() -> Array;
    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn is_existing_file(key: String) -> bool;
    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn remove_value(key: String);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn disconnect_from_peer(peer_index: BigInt) -> JsValue;

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn fetch_block_from_peer(hash: String, peer_index: BigInt, url: String) -> JsValue;
}

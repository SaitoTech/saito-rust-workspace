use std::fmt::{Debug, Formatter};
use std::io::{Error, ErrorKind};

use async_trait::async_trait;
use js_sys::{Array, BigInt, Boolean, Uint8Array};
use log::{info, trace};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

use saito_core::common::defs::{PeerIndex, SaitoHash};
use saito_core::common::interface_io::{InterfaceEvent, InterfaceIO};
use saito_core::core::data::configuration::PeerConfig;
use saito_core::core::data::transaction::Transaction;

pub struct WasmIoHandler {}

#[async_trait]
impl InterfaceIO for WasmIoHandler {
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        info!("WasmIoHandler::send_message : {:?}", peer_index);

        let array = js_sys::Uint8Array::new_with_length(buffer.len() as u32);
        array.copy_from(buffer.as_slice());

        // let async_fn =
        MsgHandler::send_message(js_sys::BigInt::from(peer_index), &array);
        // let promise = js_sys::Promise::resolve(async_fn);
        // let result = wasm_bindgen_futures::JsFuture::from(async_fn).await;
        drop(array);

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        let array = js_sys::Uint8Array::new_with_length(buffer.len() as u32);
        array.copy_from(buffer.as_slice());

        let arr2 = js_sys::Array::new_with_length(peer_exceptions.len() as u32);

        for (i, ex) in peer_exceptions.iter().enumerate() {
            let int = js_sys::BigInt::from(*ex);
            let int = JsValue::from(int);

            arr2.set(i as u32, int);
        }

        MsgHandler::send_message_to_all(&array, &arr2);

        drop(array);
        drop(arr2);

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: PeerConfig) -> Result<(), Error> {
        trace!("connect_to_peer : {:?}", peer.host);

        let json_string = serde_json::to_string(&peer).unwrap();
        let json = js_sys::JSON::parse(&json_string).unwrap();
        MsgHandler::connect_to_peer(json);

        Ok(())
    }

    // async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error> {
    //     todo!()
    // }

    async fn disconnect_from_peer(&mut self, peer_index: u64) -> Result<(), Error> {
        MsgHandler::disconnect_from_peer(js_sys::BigInt::from(peer_index));
        Ok(())
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
        let hash = js_sys::Uint8Array::new_with_length(32);
        hash.copy_from(block_hash.as_slice());
        MsgHandler::fetch_block_from_peer(&hash, BigInt::from(peer_index), url);

        drop(hash);

        Ok(())
    }

    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error> {
        let array = js_sys::Uint8Array::new_with_length(value.len() as u32);
        array.copy_from(value.as_slice());

        MsgHandler::write_value(key, &array);
        drop(array);

        Ok(())
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        let result = MsgHandler::read_value(key);
        if result.is_err() {
            return Err(Error::from(ErrorKind::Other));
        }

        let result = result.unwrap();
        let v = result.to_vec();
        drop(result);
        Ok(v)
    }

    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        let result = MsgHandler::load_block_file_list();
        if result.is_err() {
            return Err(Error::from(ErrorKind::Other));
        }

        let result = result.unwrap();
        let result = Array::try_from(result);
        if result.is_err() {
            return Err(Error::from(ErrorKind::Other));
        }
        let result = result.unwrap();

        let mut v = vec![];
        for i in 0..result.length() {
            let res = result.get(i);
            let res = js_sys::JsString::from(res).as_string().unwrap();
            v.push(res);
        }

        Ok(v)
    }

    async fn is_existing_file(&self, key: String) -> bool {
        let result = MsgHandler::is_existing_file(key);
        if result.is_err() {
            return false;
        }

        let result = result.unwrap();
        result.into()
    }

    async fn remove_value(&self, key: String) -> Result<(), Error> {
        MsgHandler::remove_value(key);

        Ok(())
    }

    fn get_block_dir(&self) -> String {
        "data/blocks/".to_string()
    }

    async fn process_api_call(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        let buf = Uint8Array::new_with_length(buffer.len() as u32);
        buf.copy_from(buffer.as_slice());
        MsgHandler::process_api_call(buf, msg_index, peer_index);
    }

    async fn process_api_success(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        // let tx = Transaction::deserialize_from_net(&buffer);
        // let buffer = tx.data;
        let buf = Uint8Array::new_with_length(buffer.len() as u32);
        buf.copy_from(buffer.as_slice());
        MsgHandler::process_api_success(buf, msg_index, peer_index);
    }

    async fn process_api_error(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        // let tx = Transaction::deserialize_from_net(&buffer);
        // let buffer = tx.data;

        let buf = Uint8Array::new_with_length(buffer.len() as u32);
        buf.copy_from(buffer.as_slice());
        MsgHandler::process_api_error(buf, msg_index, peer_index);
    }

    fn send_interface_event(&self, event: InterfaceEvent) {
        match event {
            InterfaceEvent::PeerHandshakeComplete(index) => {
                MsgHandler::send_interface_event("handshake_complete".to_string(), index);
            }
            InterfaceEvent::PeerConnectionDropped(index) => {
                MsgHandler::send_interface_event("peer_disconnect".to_string(), index);
            }
            InterfaceEvent::PeerConnected(index) => {
                MsgHandler::send_interface_event("peer_connect".to_string(), index);
            }
        }
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
    pub fn send_message(peer_index: BigInt, buffer: &Uint8Array);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn send_message_to_all(buffer: &Uint8Array, exceptions: &Array);

    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn connect_to_peer(peer_data: JsValue) -> Result<JsValue, js_sys::Error>;
    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn write_value(key: String, value: &Uint8Array);

    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn read_value(key: String) -> Result<Uint8Array, js_sys::Error>;

    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn load_block_file_list() -> Result<Array, js_sys::Error>;
    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn is_existing_file(key: String) -> Result<Boolean, js_sys::Error>;
    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn remove_value(key: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn disconnect_from_peer(peer_index: BigInt) -> Result<JsValue, js_sys::Error>;

    #[wasm_bindgen(static_method_of = MsgHandler, catch)]
    pub fn fetch_block_from_peer(
        hash: &Uint8Array,
        peer_index: BigInt,
        url: String,
    ) -> Result<JsValue, js_sys::Error>;

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn process_api_call(buffer: Uint8Array, msg_index: u32, peer_index: u64);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn process_api_success(buffer: Uint8Array, msg_index: u32, peer_index: u64);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn process_api_error(buffer: Uint8Array, msg_index: u32, peer_index: u64);

    #[wasm_bindgen(static_method_of = MsgHandler)]
    pub fn send_interface_event(event: String, peer_index: u64);
}

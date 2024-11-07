use std::fmt::{Debug, Formatter};
use std::io::Error;

use async_trait::async_trait;
use log::trace;

use saito_core::core::consensus::peers::peer_service::PeerService;
use saito_core::core::consensus::wallet::Wallet;
use saito_core::core::defs::{BlockId, PeerIndex, SaitoHash};
use saito_core::core::io::interface_io::{InterfaceEvent, InterfaceIO};


pub struct WasmIoHandler {}

#[async_trait]
impl InterfaceIO for WasmIoHandler {
    async fn send_message(&self, peer_index: u64, buffer: &[u8]) -> Result<(), Error> {
        trace!("WasmIoHandler::send_message : {:?}", peer_index);

        // let array = js_sys::Uint8Array::new_with_length(buffer.len() as u32);
        // array.copy_from(buffer);

        // let async_fn =
        // MsgHandler::send_message(js_sys::BigInt::from(peer_index), &array);
        // let promise = js_sys::Promise::resolve(async_fn);
        // let result = wasm_bindgen_futures::JsFuture::from(async_fn).await;
        // drop(array);

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        buffer: &[u8],
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        // let array = js_sys::Uint8Array::new_with_length(buffer.len() as u32);
        // array.copy_from(buffer);
        //
        // let arr2 = js_sys::Array::new_with_length(peer_exceptions.len() as u32);
        //
        // for (i, ex) in peer_exceptions.iter().enumerate() {
        //     let int = js_sys::BigInt::from(*ex);
        //     let int = JsValue::from(int);
        //
        //     arr2.set(i as u32, int);
        // }
        //
        // MsgHandler::send_message_to_all(&array, &arr2);
        //
        // drop(array);
        // drop(arr2);

        Ok(())
    }

    async fn connect_to_peer(&mut self, url: String, peer_index: PeerIndex) -> Result<(), Error> {
        trace!("connect_to_peer : {:?} with url : {:?}", peer_index, url);

        // MsgHandler::connect_to_peer(url, BigInt::from(peer_index)).expect("TODO: panic message");

        Ok(())
    }

    async fn disconnect_from_peer(&self, peer_index: u64) -> Result<(), Error> {
        trace!("disconnect from peer : {:?}", peer_index);
        // MsgHandler::disconnect_from_peer(js_sys::BigInt::from(peer_index))
        //     .expect("TODO: panic message");
        Ok(())
    }

    async fn fetch_block_from_peer(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        url: &str,
        block_id: BlockId,
    ) -> Result<(), Error> {
        // let hash = js_sys::Uint8Array::new_with_length(32);
        // hash.copy_from(block_hash.as_slice());
        // let result = MsgHandler::fetch_block_from_peer(
        //     &hash,
        //     BigInt::from(peer_index),
        //     url.to_string(),
        //     BigInt::from(block_id),
        // );
        // if result.is_err() {
        //     error!(
        //         "failed fetching block : {:?} from peer. {:?}",
        //         block_hash.to_hex(),
        //         result.err().unwrap()
        //     );
        //     return Err(Error::from(ErrorKind::Other));
        // }

        Ok(())
    }

    async fn write_value(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        // let array = js_sys::Uint8Array::new_with_length(value.len() as u32);
        // array.copy_from(value);
        //
        // MsgHandler::write_value(key.to_string(), &array);
        // drop(array);

        Ok(())
    }

    async fn append_value(&mut self, key: &str, value: &[u8]) -> Result<(), Error> {
        // let array = js_sys::Uint8Array::new_with_length(value.len() as u32);
        // array.copy_from(value);
        //
        // MsgHandler::append_value(key.to_string(), &array);
        // drop(array);

        Ok(())
    }

    async fn flush_data(&mut self, key: &str) -> Result<(), Error> {
        // MsgHandler::flush_data(key.to_string());

        Ok(())
    }

    async fn read_value(&self, key: &str) -> Result<Vec<u8>, Error> {
        // let result = MsgHandler::read_value(key.to_string());
        // if result.is_err() {
        //     error!("couldn't read value for key: {:?}", key);
        //     return Err(Error::from(ErrorKind::Other));
        // }
        //
        // let result = result.unwrap();
        // let v = result.to_vec();
        // drop(result);
        // Ok(v)
        todo!()
    }

    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        // let result = MsgHandler::load_block_file_list();
        // if result.is_err() {
        //     return Err(Error::from(ErrorKind::Other));
        // }
        //
        // let result = result.unwrap();
        // let result = Array::try_from(result);
        // if result.is_err() {
        //     return Err(Error::from(ErrorKind::Other));
        // }
        // let result = result.unwrap();
        //
        // let mut v = vec![];
        // for i in 0..result.length() {
        //     let res = result.get(i);
        //     let res = js_sys::String::from(res).as_string().unwrap();
        //     v.push(res);
        // }
        //
        // Ok(v)
        todo!()
    }

    async fn is_existing_file(&self, key: &str) -> bool {
        // let result = MsgHandler::is_existing_file(key.to_string());
        // if result.is_err() {
        return false;
        // }
        //
        // let result = result.unwrap();
        // result.into()
    }

    async fn remove_value(&self, key: &str) -> Result<(), Error> {
        // let _ = MsgHandler::remove_value(key.to_string());

        Ok(())
    }

    fn get_block_dir(&self) -> String {
        "data/blocks/".to_string()
    }

    fn ensure_block_directory_exists(&self, block_dir_path: &str) -> Result<(), std::io::Error> {
        // let result = MsgHandler::ensure_block_directory_exists(block_dir_path.to_string());
        // if result.is_err() {
        //     error!("{:?}", result.err().unwrap());
        //     return Err(Error::from(ErrorKind::Other));
        // }
        Ok(())
    }

    async fn process_api_call(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        // let buf = Uint8Array::new_with_length(buffer.len() as u32);
        // buf.copy_from(buffer.as_slice());
        // MsgHandler::process_api_call(buf, msg_index, BigInt::from(peer_index));
    }

    async fn process_api_success(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        // let tx = Transaction::deserialize_from_net(&buffer);
        // let buffer = tx.data;
        // let buf = Uint8Array::new_with_length(buffer.len() as u32);
        // buf.copy_from(buffer.as_slice());
        // MsgHandler::process_api_success(buf, msg_index, BigInt::from(peer_index));
    }

    async fn process_api_error(&self, buffer: Vec<u8>, msg_index: u32, peer_index: PeerIndex) {
        // let tx = Transaction::deserialize_from_net(&buffer);
        // let buffer = tx.data;

        // let buf = Uint8Array::new_with_length(buffer.len() as u32);
        // buf.copy_from(buffer.as_slice());
        // MsgHandler::process_api_error(buf, msg_index, BigInt::from(peer_index));
    }

    fn send_interface_event(&self, event: InterfaceEvent) {
        // match event {
        //     InterfaceEvent::PeerHandshakeComplete(index) => {
        //         MsgHandler::send_interface_event(
        //             "handshake_complete".to_string(),
        //             BigInt::from(index),
        //             "".to_string(),
        //         );
        //     }
        //     InterfaceEvent::PeerConnectionDropped(index, public_key) => {
        //         MsgHandler::send_interface_event(
        //             "peer_disconnect".to_string(),
        //             BigInt::from(index),
        //             public_key.to_base58(),
        //         );
        //     }
        //     InterfaceEvent::PeerConnected(index) => {
        //         MsgHandler::send_interface_event(
        //             "peer_connect".to_string(),
        //             BigInt::from(index),
        //             "".to_string(),
        //         );
        //     }
        //     InterfaceEvent::BlockAddSuccess(hash, block_id) => {
        //         MsgHandler::send_block_success(hash.to_hex(), BigInt::from(block_id));
        //     }
        //     InterfaceEvent::WalletUpdate() => {
        //         MsgHandler::send_wallet_update();
        //     }
        //     InterfaceEvent::NewVersionDetected(index, version) => {
        //         MsgHandler::send_new_version_alert(
        //             format!(
        //                 "{:?}.{:?}.{:?}",
        //                 version.major, version.minor, version.patch
        //             )
        //             .to_string(),
        //             BigInt::from(index),
        //         );
        //     }
        //
        //     InterfaceEvent::StunPeerConnected(index) => {
        //         MsgHandler::send_interface_event(
        //             "stun peer connect".to_string(),
        //             BigInt::from(index),
        //             "".to_string(),
        //         );
        //     }
        //     InterfaceEvent::StunPeerDisconnected(index, public_key) => {
        //         MsgHandler::send_interface_event(
        //             "stun peer disconnect".to_string(),
        //             BigInt::from(index),
        //             public_key.to_base58(),
        //         );
        //     }
        //     InterfaceEvent::BlockFetchStatus(count) => {
        //         MsgHandler::send_block_fetch_status_event(count);
        //     }
        // }
    }

    async fn save_wallet(&self, _wallet: &mut Wallet) -> Result<(), Error> {
        // TODO : return error state
        Ok(())
    }

    async fn load_wallet(&self, _wallet: &mut Wallet) -> Result<(), Error> {
        // TODO : return error state
        Ok(())
    }

    // async fn save_blockchain(&self) -> Result<(), Error> {
    //     MsgHandler::save_blockchain();
    //     // TODO : return error state
    //     Ok(())
    // }
    //
    // async fn load_blockchain(&self) -> Result<(), Error> {
    //     MsgHandler::load_blockchain();
    //     // TODO : return error state
    //     Ok(())
    // }

    fn get_my_services(&self) -> Vec<PeerService> {
        todo!()
    }
}

impl Debug for WasmIoHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustIoHandler")
            // .field("handler_id", &self.handler_id)
            .finish()
    }
}

use std::io::Error;

use async_trait::async_trait;
use saito_core::common::handle_io::HandleIo;

pub struct WasmIoHandler {}

#[async_trait]
impl HandleIo for WasmIoHandler {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }

    async fn get_timestamp(&self) -> u64 {
        todo!()
    }
}

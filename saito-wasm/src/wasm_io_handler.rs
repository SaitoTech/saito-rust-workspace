use std::io::Error;

use async_trait::async_trait;

use saito_core::common::command::InterfaceEvent;
use saito_core::common::handle_io::HandleIo;
use saito_core::core::data::configuration::Peer;

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

    async fn send_message_to_all(
        &self,
        message_name: String,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn connect_to_peer(&mut self, peer: Peer) -> Result<(), Error> {
        todo!()
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(
        &mut self,
        result_key: String,
        key: String,
        value: Vec<u8>,
    ) -> Result<String, Error> {
        todo!()
    }

    fn set_write_result(
        &mut self,
        result_key: String,
        result: Result<String, Error>,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
}

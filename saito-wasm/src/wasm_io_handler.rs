use std::io::Error;

use async_trait::async_trait;

use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::SaitoHash;
use saito_core::common::interface_io::InterfaceIO;
use saito_core::core::data::block::Block;
use saito_core::core::data::configuration::PeerConfig;

pub struct WasmIoHandler {}

#[async_trait]
impl InterfaceIO for WasmIoHandler {
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
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

    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    // fn set_write_result(
    //     &mut self,
    //     result_key: String,
    //     result: Result<String, Error>,
    // ) -> Result<(), Error> {
    //     todo!()
    // }

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

    async fn disconnect_from_peer(&mut self, peer_index: u64) -> Result<(), Error> {
        todo!()
    }

    async fn fetch_block_from_peer(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
    ) -> Result<Block, Error> {
        todo!()
    }
}

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
        "test_data/blocks/".to_string()
    }

    async fn fetch_block_by_url(&self, url: String) -> Result<Block, Error> {
        todo!()
    }
}

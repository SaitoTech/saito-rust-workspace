use std::io::Error;

use async_trait::async_trait;

use crate::common::defs::SaitoHash;
use crate::core::data;
use crate::core::data::block::Block;

#[async_trait]
pub trait HandleIo {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error>;

    /// Sends the given message buffer to all the peers except the ones specified
    ///
    /// # Arguments
    ///
    /// * `message_name`:
    /// * `buffer`:
    /// * `peer_exceptions`: Peer indices for which this message should not be sent
    ///
    /// returns: Result<(), Error>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn send_message_to_all(
        &self,
        message_name: String,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error>;
    async fn connect_to_peer(&mut self, peer: data::configuration::Peer) -> Result<(), Error>;
    // async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error>;
    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error>;
    // fn set_write_result(
    //     &mut self,
    //     result_key: String,
    //     result: Result<String, Error>,
    // ) -> Result<(), Error>;
    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;
    async fn load_block_file_list(&self) -> Result<Vec<String>, Error>;
    async fn is_existing_file(&self, key: String) -> bool;
    async fn remove_value(&self, key: String) -> Result<(), Error>;
    fn get_block_dir(&self) -> String;
    async fn fetch_block_from_peer(&self, url: String) -> Result<Block, Error>;
}

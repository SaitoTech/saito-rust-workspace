use std::io::Error;

use async_trait::async_trait;

use crate::common::defs::SaitoHash;
use crate::core::data;
use crate::core::data::block::Block;
use crate::core::data::msg::message::Message;

#[async_trait]
pub trait HandleIo {
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error>;

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
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error>;
    async fn connect_to_peer(&mut self, peer: data::configuration::Peer) -> Result<(), Error>;
    async fn disconnect_from_peer(&mut self, peer_index: u64) -> Result<(), Error>;
    async fn fetch_block_from_peer(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
    ) -> Result<(), Error>;

    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error>;
    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;
    async fn load_block_file_list(&self) -> Result<Vec<String>, Error>;
    async fn is_existing_file(&self, key: String) -> bool;
    async fn remove_value(&self, key: String) -> Result<(), Error>;
    fn get_block_dir(&self) -> String;
}

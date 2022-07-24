use crate::core::data::block::Block;
use std::io::Error;

use async_trait::async_trait;

use crate::common::defs::SaitoHash;
use crate::core::data;

/// An interface is provided to access the IO functionalities in a platform (Rust/WASM) agnostic way
#[async_trait]
pub trait InterfaceIO {
    /// Fetches a block with given hash from a specific peer
    ///
    /// # Arguments
    ///
    /// * `block_hash`:
    /// * `peer_index`:
    /// * `url`:
    ///
    /// returns: Result<(), Error>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn fetch_block_by_url(&self, url: String) -> Result<Block, Error>;

    /// Writes a value to a persistent storage with the given key
    ///
    /// # Arguments
    ///
    /// * `key`:
    /// * `value`:
    ///
    /// returns: Result<(), Error>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error>;
    /// Reads a value with the given key from a persistent storage
    ///
    /// # Arguments
    ///
    /// * `key`:
    ///
    /// returns: Result<Vec<u8, Global>, Error>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;

    /// Loads the block path list from the persistent storage
    async fn load_block_file_list(&self) -> Result<Vec<String>, Error>;
    async fn is_existing_file(&self, key: String) -> bool;
    /// Removes the value with the given key from the persistent storage
    async fn remove_value(&self, key: String) -> Result<(), Error>;
    /// Retrieve the prefix for all the keys for blocks
    fn get_block_dir(&self) -> String;
}

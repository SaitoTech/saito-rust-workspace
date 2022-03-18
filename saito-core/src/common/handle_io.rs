use std::io::Error;

use async_trait::async_trait;

#[async_trait]
pub trait HandleIo {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error>;

    async fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error>;
    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;

    async fn get_timestamp(&self) -> u64;
}

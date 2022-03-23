use std::io::Error;

use async_trait::async_trait;

use crate::common::command::InterfaceEvent;

#[async_trait]
pub trait HandleIo {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error>;
    async fn process_interface_event(&mut self, event: InterfaceEvent);
    async fn write_value(
        &mut self,
        result_key: String,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), Error>;
    fn set_write_result(&mut self, result_key: String, result: Result<String, Error>);
    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error>;

    fn get_timestamp(&self) -> u64;
}

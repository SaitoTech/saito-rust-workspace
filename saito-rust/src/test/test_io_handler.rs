use std::fs;
use std::io::Error;

use async_trait::async_trait;

use log::debug;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use saito_core::common::handle_io::HandleIo;

use saito_core::core::data::configuration::Peer;

#[derive(Clone, Debug)]
pub struct TestIOHandler {}

impl TestIOHandler {
    pub fn new() -> TestIOHandler {
        TestIOHandler {}
    }
}

#[async_trait]
impl HandleIo for TestIOHandler {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error> {
        // TODO : implement a way to check sent messages

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        message_name: String,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        debug!("send message to all {:?}", message_name);

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: Peer) -> Result<(), Error> {
        debug!("connecting to peer : {:?}", peer.host);

        Ok(())
    }

    async fn write_value(&mut self, key: String, value: Vec<u8>) -> Result<(), Error> {
        debug!("writing value to disk : {:?}", key);
        let filename = "./data/".to_string() + key.as_str();
        let result = File::create(filename).await;
        if result.is_err() {
            return Err(result.err().unwrap());
        }
        let mut file = result.unwrap();
        let result = file.write_all(&value).await;
        if result.is_err() {
            return Err(result.err().unwrap());
        }

        Ok(())
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }

    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        let mut paths: Vec<_> = fs::read_dir("./data/blocks")
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        paths.sort_by(|a, b| {
            let a_metadata = fs::metadata(a.path()).unwrap();
            let b_metadata = fs::metadata(b.path()).unwrap();
            a_metadata
                .modified()
                .unwrap()
                .partial_cmp(&b_metadata.modified().unwrap())
                .unwrap()
        });
        let mut filenames = vec![];
        for entry in paths {
            filenames.push(entry.file_name().into_string().unwrap());
        }

        Ok(filenames)
    }
}

use std::fs;
use std::io::Error;
use std::path::Path;

use async_trait::async_trait;

use log::{debug, info};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        let filename = key.as_str();
        let path = Path::new(filename);
        if path.parent().is_some() {
            tokio::fs::create_dir_all(path.parent().unwrap())
                .await
                .expect("creating directory structure failed");
        }
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
        let mut result = File::open(key).await;
        if result.is_err() {
            todo!()
        }
        let mut file = result.unwrap();
        let mut encoded = Vec::<u8>::new();

        let result = file.read_to_end(&mut encoded).await;
        if result.is_err() {
            todo!()
        }
        Ok(encoded)
    }

    async fn load_block_file_list(&self) -> Result<Vec<String>, Error> {
        info!("current dir = {:?}", std::env::current_dir().unwrap());
        let mut paths: Vec<_> = fs::read_dir("data/blocks")
            .unwrap()
            .map(|r| r.unwrap())
            .filter(|r| r.file_name().into_string().unwrap().contains(".block"))
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
    async fn is_existing_file(&self, key: String) -> bool {
        let result = tokio::fs::File::open(key).await;
        if result.is_ok() {
            return true;
        }
        return false;
    }
    async fn remove_value(&self, key: String) -> Result<(), Error> {
        let result = tokio::fs::remove_file(key).await;
        return result;
    }
}

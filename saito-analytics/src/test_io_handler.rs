use std::fs;
use std::io::Error;
use std::path::Path;

use async_trait::async_trait;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use log::{debug, error, info};

use saito_core::common::defs::{PeerIndex, SaitoHash, BLOCK_FILE_EXTENSION};
use saito_core::common::interface_io::{InterfaceEvent, InterfaceIO};

use saito_core::core::data::configuration::PeerConfig;
use saito_core::core::data::peer_service::PeerService;
use saito_core::core::data::wallet::Wallet;

#[derive(Clone, Debug)]
pub struct TestIOHandler {}

impl TestIOHandler {
    pub fn new() -> TestIOHandler {
        TestIOHandler {}
    }
}

#[async_trait]
impl InterfaceIO for TestIOHandler {
    async fn send_message(&self, _peer_index: u64, _buffer: Vec<u8>) -> Result<(), Error> {
        // TODO : implement a way to check sent messages

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        _buffer: Vec<u8>,
        _peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        debug!("send message to all");

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: PeerConfig) -> Result<(), Error> {
        debug!("connecting to peer : {:?}", peer.host);

        Ok(())
    }

    async fn disconnect_from_peer(&mut self, _peer_index: u64) -> Result<(), Error> {
        todo!("")
    }

    async fn fetch_block_from_peer(
        &self,
        _block_hash: SaitoHash,
        _peer_index: u64,
        _url: String,
    ) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
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
        file.flush().await.expect("flush failed");
        Ok(())
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        let result = File::open(key.clone()).await;
        if result.is_err() {
            error!(
                "error : path {:?} \r\n : {:?}",
                key.to_string(),
                result.err().unwrap()
            );
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
        let result = fs::read_dir(self.get_block_dir());
        if result.is_err() {
            return Err(result.err().unwrap());
        }
        let mut paths: Vec<_> = result
            .unwrap()
            .map(|r| r.unwrap())
            .filter(|r| {
                r.file_name()
                    .into_string()
                    .unwrap()
                    .contains(BLOCK_FILE_EXTENSION)
            })
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

    fn get_block_dir(&self) -> String {
        "./data/blocks/".to_string()
    }

    async fn process_api_call(&self, _buffer: Vec<u8>, _msg_index: u32, _peer_index: PeerIndex) {
        todo!()
    }

    async fn process_api_success(&self, _buffer: Vec<u8>, _msg_index: u32, _peer_index: PeerIndex) {
        todo!()
    }

    async fn process_api_error(&self, _buffer: Vec<u8>, _msg_index: u32, _peer_index: PeerIndex) {
        todo!()
    }

    fn send_interface_event(&self, _event: InterfaceEvent) {}

    async fn save_wallet(&self, wallet: &mut Wallet) -> Result<(), Error> {
        todo!()
    }

    async fn load_wallet(&self, wallet: &mut Wallet) -> Result<(), Error> {
        todo!()
    }

    async fn save_blockchain(&self) -> Result<(), Error> {
        todo!()
    }

    async fn load_blockchain(&self) -> Result<(), Error> {
        todo!()
    }

    fn get_my_services(&self) -> Vec<PeerService> {
        todo!()
    }
}

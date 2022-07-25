use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Mutex;

use crate::saito::io_context::IoContext;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, error, warn};
use saito_core::common::interface_io::InterfaceIO;
use saito_core::core::data::block::Block;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

lazy_static! {
    pub static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
    pub static ref BLOCKS_DIR_PATH: String = configure_storage();
}
pub fn configure_storage() -> String {
    if cfg!(test) {
        String::from("./test_data/test/blocks/")
    } else {
        String::from("./test_data/blocks/")
    }
}

pub enum FutureState {
    DataSent(Vec<u8>),
    PeerConnectionResult(Result<u64, Error>),
}

#[derive(Clone, Debug)]
pub struct RustIOHandler {}

impl RustIOHandler {
    pub fn new() -> RustIOHandler {
        RustIOHandler {}
    }

    // TODO : delete this if not required
    pub fn set_event_response(event_id: u64, response: FutureState) {
        // debug!("setting event response for : {:?}", event_id,);
        if event_id == 0 {
            return;
        }
        let waker;
        {
            let mut context = SHARED_CONTEXT.lock().unwrap();
            context.future_states.insert(event_id, response);
            waker = context.future_wakers.remove(&event_id);
        }
        if waker.is_some() {
            // debug!("waking future on event: {:?}", event_id,);
            let waker = waker.unwrap();
            waker.wake();
            // debug!("waker invoked on event: {:?}", event_id);
        } else {
            warn!("waker not found for event: {:?}", event_id);
        }
    }
}

#[async_trait]
impl InterfaceIO for RustIOHandler {
    async fn fetch_block_by_url(&self, url: String) -> Result<Block, Error> {
        debug!("fetching block : {:?}", url);

        let result = reqwest::get(url.clone()).await;
        if result.is_err() {
            error!(
                "fetching failed : {:?}, {:?}",
                url,
                result.err().unwrap().to_string()
            );
            return Err(Error::from(ErrorKind::ConnectionRefused));
        }
        let response = result.unwrap();
        let result = response.bytes().await;
        if result.is_err() {
            error!(
                "binary conversion failed : {:?}, {:?}",
                url,
                result.err().unwrap().to_string()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }

        let result = result.unwrap();
        let buffer = result.to_vec();
        let block = Block::deserialize_from_net(&buffer);
        debug!("block buffer received");

        Ok(block)
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
        let result = File::open(key).await;
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
        debug!(
            "loading blocks from dir : {:?}",
            self.get_block_dir().to_string(),
        );
        let result = fs::read_dir(self.get_block_dir());
        if result.is_err() {
            debug!("no blocks found");
            return Err(result.err().unwrap());
        }
        let mut paths: Vec<_> = result
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
        return Path::new(&key).exists();
    }

    async fn remove_value(&self, key: String) -> Result<(), Error> {
        let result = tokio::fs::remove_file(key).await;
        return result;
    }

    fn get_block_dir(&self) -> String {
        BLOCKS_DIR_PATH.to_string()
    }
}

#[cfg(test)]
mod tests {
    use saito_core::common::interface_io::InterfaceIO;

    use crate::saito::rust_io_handler::RustIOHandler;

    #[tokio::test]
    async fn test_write_value() {
        let mut io_handler = RustIOHandler::new();

        let result = io_handler
            .write_value("./test_data/test/KEY".to_string(), [1, 2, 3, 4].to_vec())
            .await;
        assert!(result.is_ok(), "{:?}", result.err().unwrap().to_string());
        let result = io_handler
            .read_value("./test_data/test/KEY".to_string())
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result, [1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn file_exists_success() {
        let mut io_handler = RustIOHandler::new();
        let path = String::from("src/test/test_data/config_handler_tests.json");

        let result = io_handler.is_existing_file(path).await;
        assert!(result);
    }

    #[tokio::test]
    async fn file_exists_fail() {
        let mut io_handler = RustIOHandler::new();
        let path = String::from("badfilename.json");

        let result = io_handler.is_existing_file(path).await;
        assert!(!result);
    }
}

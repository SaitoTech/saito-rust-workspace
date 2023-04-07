use std::fmt::{Debug, Formatter};
use std::fs;
use std::io::Error;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use lazy_static::lazy_static;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Sender;

use log::{debug, warn};
use saito_core::common::command::NetworkEvent;
use saito_core::common::defs::{PeerIndex, SaitoHash, BLOCK_FILE_EXTENSION};
use saito_core::common::interface_io::InterfaceIO;
use saito_core::core::data::configuration::PeerConfig;

// use crate::saito::io_context::IoContext;
use crate::IoEvent;

lazy_static! {
    // pub static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
    pub static ref BLOCKS_DIR_PATH: String = configure_storage();
}
pub fn configure_storage() -> String {
    if cfg!(test) {
        String::from("./data/test/blocks/")
    } else {
        String::from("./data/blocks/")
    }
}

// pub enum FutureState {
//     DataSent(Vec<u8>),
//     PeerConnectionResult(Result<u64, Error>),
// }

pub struct RustIOHandler {
    sender: Sender<IoEvent>,
    handler_id: u8,
}

impl RustIOHandler {
    pub fn new(sender: Sender<IoEvent>, handler_id: u8) -> RustIOHandler {
        RustIOHandler { sender, handler_id }
    }

    // // TODO : delete this if not required
    // pub fn set_event_response(event_id: u64, response: FutureState) {
    //     // debug!("setting event response for : {:?}", event_id,);
    //     if event_id == 0 {
    //         return;
    //     }
    //     let waker;
    //     {
    //         let mut context = SHARED_CONTEXT.lock().unwrap();
    //         context.future_states.insert(event_id, response);
    //         waker = context.future_wakers.remove(&event_id);
    //     }
    //     if waker.is_some() {
    //         // debug!("waking future on event: {:?}", event_id,);
    //         let waker = waker.unwrap();
    //         waker.wake();
    //         // debug!("waker invoked on event: {:?}", event_id);
    //     } else {
    //         warn!("waker not found for event: {:?}", event_id);
    //     }
    // }
}

impl Debug for RustIOHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustIoHandler")
            .field("handler_id", &self.handler_id)
            .finish()
    }
}

#[async_trait]
impl InterfaceIO for RustIOHandler {
    async fn send_message(&self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        // TODO : refactor to combine event and the future
        let event = IoEvent::new(NetworkEvent::OutgoingNetworkMessage { peer_index, buffer });

        self.sender.send(event).await.unwrap();

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        debug!("send message to all");

        let event = IoEvent::new(NetworkEvent::OutgoingNetworkMessageForAll {
            buffer,
            exceptions: peer_exceptions,
        });

        self.sender.send(event).await.unwrap();

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: PeerConfig) -> Result<(), Error> {
        debug!("connecting to peer : {:?}", peer.host);
        let event = IoEvent::new(NetworkEvent::ConnectToPeer {
            peer_details: peer.clone(),
        });

        self.sender.send(event).await.unwrap();

        Ok(())
    }

    async fn disconnect_from_peer(&mut self, _peer_index: u64) -> Result<(), Error> {
        todo!()
    }

    async fn fetch_block_from_peer(
        &self,
        block_hash: SaitoHash,
        peer_index: u64,
        url: String,
    ) -> Result<(), Error> {
        if block_hash == [0; 32] {
            return Ok(());
        }

        debug!("fetching block from peer : {:?}", url);
        let event = IoEvent::new(NetworkEvent::BlockFetchRequest {
            block_hash,
            peer_index,
            url: url.clone(),
        });

        self.sender
            .send(event)
            .await
            .expect("failed sending to io controller");

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
        return Path::new(&key).exists();
    }

    async fn remove_value(&self, key: String) -> Result<(), Error> {
        let result = tokio::fs::remove_file(key).await;
        return result;
    }

    fn get_block_dir(&self) -> String {
        BLOCKS_DIR_PATH.to_string()
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
}

#[cfg(test)]
mod tests {
    use saito_core::common::interface_io::InterfaceIO;

    use crate::saito::rust_io_handler::RustIOHandler;

    #[tokio::test]
    async fn test_write_value() {
        let (sender, mut _receiver) = tokio::sync::mpsc::channel(10);
        let mut io_handler = RustIOHandler::new(sender, 0);

        let result = io_handler
            .write_value("./data/test/KEY".to_string(), [1, 2, 3, 4].to_vec())
            .await;
        assert!(result.is_ok(), "{:?}", result.err().unwrap().to_string());
        let result = io_handler.read_value("./data/test/KEY".to_string()).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result, [1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn file_exists_success() {
        let (sender, mut _receiver) = tokio::sync::mpsc::channel(10);
        let io_handler = RustIOHandler::new(sender, 0);
        let path = String::from("src/test/data/config_handler_tests.json");

        let result = io_handler.is_existing_file(path).await;
        assert!(result);
    }

    #[tokio::test]
    async fn file_exists_fail() {
        let (sender, mut _receiver) = tokio::sync::mpsc::channel(10);
        let io_handler = RustIOHandler::new(sender, 0);
        let path = String::from("badfilename.json");

        let result = io_handler.is_existing_file(path).await;
        assert!(!result);
    }
}

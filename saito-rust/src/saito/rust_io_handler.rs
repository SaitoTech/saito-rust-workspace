use std::io::{Error, ErrorKind};
use std::sync::Mutex;

use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, warn};
use tokio::sync::mpsc::Sender;

use saito_core::common::command::InterfaceEvent;
use saito_core::common::handle_io::HandleIo;
use saito_core::core::data::block::Block;
use saito_core::core::data::configuration::Peer;

use crate::saito::io_context::IoContext;
use crate::saito::io_future::IoFuture;
use crate::IoEvent;

lazy_static! {
    pub static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
}

pub enum FutureState {
    DataSaved(Result<String, std::io::Error>),
    DataSent(Vec<u8>),
    BlockFetched(Block),
    PeerConnectionResult(Result<u64, std::io::Error>),
}

#[derive(Clone)]
pub struct RustIOHandler {
    sender: Sender<IoEvent>,
    // handler_id: u8,
    // future_index_counter: u64,
}

impl RustIOHandler {
    pub fn new(sender: Sender<IoEvent>) -> RustIOHandler {
        RustIOHandler {
            sender,
            // handler_id,
            // future_index_counter: 0,
        }
    }

    pub fn set_event_response(event_id: u64, response: FutureState) {
        debug!("setting event response for : {:?}", event_id,);
        if event_id == 0 {
            return;
        }
        let mut context = SHARED_CONTEXT.lock().unwrap();
        context.future_states.insert(event_id, response);
        let waker = context.future_wakers.remove(&event_id);
        if waker.is_some() {
            debug!("waking future on event: {:?}", event_id,);
            let waker = waker.unwrap();
            waker.wake();
        } else {
            warn!("waker not found for event: {:?}", event_id);
        }
    }

    // fn get_next_future_index(&mut self) -> u64 {
    //     self.future_index_counter = self.future_index_counter + 1;
    //     return self.future_index_counter;
    // }
}

#[async_trait]
impl HandleIo for RustIOHandler {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error> {
        // TODO : refactor to combine event and the future
        let event = IoEvent::new(InterfaceEvent::OutgoingNetworkMessage {
            peer_index,
            message_name,
            buffer,
        });
        let io_future = IoFuture {
            event_id: event.event_id,
        };
        self.sender.send(event).await.unwrap();

        let result = io_future.await;
        if result.is_err() {
            // warn!("sending message failed : {:?}", result.err().unwrap());
            return Err(result.err().unwrap());
        }
        let result = result.unwrap();
        match result {
            FutureState::DataSent(data) => {}
            _ => {
                unreachable!()
            }
        }
        Ok(())
    }

    async fn send_message_to_all(
        &self,
        message_name: String,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        debug!("send message to all {:?}", message_name);

        let event = IoEvent::new(InterfaceEvent::OutgoingNetworkMessageForAll {
            message_name,
            buffer,
            exceptions: peer_exceptions,
        });
        let io_future = IoFuture {
            event_id: event.event_id,
        };
        self.sender.send(event).await.unwrap();

        let result = io_future.await;
        if result.is_err() {
            // warn!("sending message failed : {:?}", result.err().unwrap());
            return Err(result.err().unwrap());
        }
        debug!("message sent to all");
        let result = result.unwrap();
        match result {
            FutureState::DataSent(data) => {}
            _ => {
                unreachable!()
            }
        }
        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: Peer) -> Result<(), Error> {
        debug!("connecting to peer : {:?}", peer.host);
        let event = IoEvent::new(InterfaceEvent::ConnectToPeer {
            peer_details: peer.clone(),
        });
        let io_future = IoFuture {
            event_id: event.event_id,
        };
        self.sender.send(event).await.unwrap();
        let result = io_future.await;
        if result.is_err() {
            warn!("failed connecting to peer : {:?}", peer.host);
            return Err(result.err().unwrap());
        }
        Ok(())
    }
    //
    // async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error> {
    //     todo!()
    // }

    async fn write_value(
        &mut self,
        result_key: String,
        key: String,
        value: Vec<u8>,
    ) -> Result<String, Error> {
        debug!("writing value to disk : {:?}", key);

        let event = IoEvent::new(InterfaceEvent::DataSaveRequest {
            key: result_key,
            filename: key,
            buffer: value,
        });
        let io_future = IoFuture {
            event_id: event.event_id,
        };
        let result = self.sender.send(event).await;

        if result.is_err() {
            warn!("{:?}", result.err().unwrap().to_string());
            return Err(Error::from(ErrorKind::Other));
        }

        let result = io_future.await;
        if result.is_err() {
            debug!("failed writing value for disk");
            return Err(result.err().unwrap());
        }
        debug!("value written to disk");
        let result = result.unwrap();
        match result {
            FutureState::DataSaved(result) => {
                return result;
            }
            _ => {
                unreachable!()
            }
        }
    }
    //
    // fn set_write_result(
    //     &mut self,
    //     result_key: String,
    //     result: Result<String, Error>,
    // ) -> Result<(), Error> {
    //     debug!("setting write result");
    //     // let mut states = self.future_states.lock().unwrap();
    //     // let mut wakers = self.future_wakers.lock().unwrap();
    //     // let waker = wakers.remove(&index).expect("waker not found");
    //     // states.insert(index, FutureState::DataSaved(result));
    //     // waker.wake();
    //     todo!()
    // }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use saito_core::common::handle_io::HandleIo;

    use crate::saito::rust_io_handler::{FutureState, RustIOHandler};
    use crate::IoEvent;

    #[tokio::test]
    async fn test_write_value() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(10);
        let mut io_handler = RustIOHandler::new(sender, 0);

        tokio::spawn(async move {
            let result = receiver.recv().await;

            let event = result.unwrap();
            RustIOHandler::set_event_response(
                event.event_id,
                FutureState::DataSaved(Ok("RESULT".to_string())),
            );
        });

        let result = io_handler
            .write_value("KEY".to_string(), "TEST".to_string(), [1, 2, 3, 4].to_vec())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "RESULT");
    }
}

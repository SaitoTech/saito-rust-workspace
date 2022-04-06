use std::collections::HashMap;
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, warn};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use saito_core::common::command::InterfaceEvent;
use saito_core::common::handle_io::HandleIo;
use saito_core::core::data::block::Block;
use saito_core::core::data::configuration::Peer;

use crate::IoEvent;

lazy_static! {
    static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
}

struct IoContext {
    pub future_wakers: HashMap<u64, Waker>,
    pub future_states: HashMap<u64, FutureState>,
}

impl IoContext {
    fn new() -> IoContext {
        IoContext {
            future_wakers: Default::default(),
            future_states: Default::default(),
        }
    }
}

pub enum FutureState {
    DataSaved(Result<String, std::io::Error>),
    DataSent(),
    BlockFetched(Block),
    PeerConnectionResult(Result<u64, std::io::Error>),
}

pub struct IoFuture {
    pub event_id: u64,
}

#[derive(Clone)]
pub struct RustIOHandler {
    sender: Sender<IoEvent>,
    handler_id: u8,
    future_index_counter: u64,
}

impl RustIOHandler {
    pub fn new(sender: Sender<IoEvent>, handler_id: u8) -> RustIOHandler {
        RustIOHandler {
            sender,
            handler_id,
            future_index_counter: 0,
        }
    }

    pub fn set_event_response(event_id: u64, response: FutureState) {
        debug!("setting event response for : {:?}", event_id);
        if event_id == 0 {
            return;
        }
        let mut context = SHARED_CONTEXT.lock().unwrap();
        context.future_states.insert(event_id, response);
        let waker = context
            .future_wakers
            .remove(&event_id)
            .expect("waker not found");
        debug!("waking future : {:?}", event_id);
        waker.wake();
    }
    fn get_next_future_index(&mut self) -> u64 {
        self.future_index_counter = self.future_index_counter + 1;
        return self.future_index_counter;
    }
}

#[async_trait]
impl HandleIo for RustIOHandler {
    async fn send_message(
        &self,
        peer_index: u64,
        message_name: String,
        buffer: Vec<u8>,
    ) -> Result<(), Error> {
        let result = self
            .sender
            .send(IoEvent::new(InterfaceEvent::OutgoingNetworkMessage {
                peer_index,
                message_name,
                buffer,
            }))
            .await;

        Ok(())
    }

    async fn send_message_to_all(
        &self,
        message_name: String,
        buffer: Vec<u8>,
        peer_exceptions: Vec<u64>,
    ) -> Result<(), Error> {
        self.sender
            .send(IoEvent::new(InterfaceEvent::OutgoingNetworkMessageForAll {
                message_name,
                buffer,
                exceptions: peer_exceptions,
            }))
            .await;
        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: Peer) -> Result<(), Error> {
        self.sender
            .send(IoEvent::new(InterfaceEvent::ConnectToPeer {
                peer_details: peer,
            }))
            .await;
        Ok(())
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Result<(), Error> {
        todo!()
    }

    async fn write_value(
        &mut self,
        result_key: String,
        key: String,
        value: Vec<u8>,
    ) -> Result<String, Error> {
        debug!("writing value to disk : {:?}", key);
        let io_future = IoFuture {
            event_id: self.get_next_future_index(),
        };
        let result = self
            .sender
            .send(IoEvent::new(InterfaceEvent::DataSaveRequest {
                key: result_key,
                filename: key,
                buffer: value,
            }))
            .await;

        if result.is_err() {
            warn!("{:?}", result.err().unwrap().to_string());
            return Err(Error::from(ErrorKind::Other));
        }

        debug!("waiting for future to complete");
        let result = io_future.await;
        if result.is_err() {
            debug!("failed writing value for disk");
            return Err(result.err().unwrap());
        }
        debug!("future is complete");
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

    fn set_write_result(
        &mut self,
        result_key: String,
        result: Result<String, Error>,
    ) -> Result<(), Error> {
        debug!("setting write result");
        // let mut states = self.future_states.lock().unwrap();
        // let mut wakers = self.future_wakers.lock().unwrap();
        // let waker = wakers.remove(&index).expect("waker not found");
        // states.insert(index, FutureState::DataSaved(result));
        // waker.wake();
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }
}

impl Future for IoFuture {
    type Output = Result<FutureState, std::io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut context = SHARED_CONTEXT.lock().unwrap();

        let result = context.future_states.remove(&self.event_id);
        if result.is_none() {
            context
                .future_wakers
                .insert(self.event_id, cx.waker().clone());
            debug!("waiting for event : {:?}", self.event_id);
            return Poll::Pending;
        }
        Poll::Ready(Ok(result.unwrap()))
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

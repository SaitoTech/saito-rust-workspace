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

// lazy_static! {
//     static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
// }

struct IoContext {
    awaiting_requests: HashMap<u64, bool>,
    request_counter: u64,
}

impl IoContext {
    fn new() -> IoContext {
        IoContext {
            awaiting_requests: Default::default(),
            request_counter: 0,
        }
    }
    fn get_next_request_index(&mut self) -> u64 {
        // TODO : check if the next index is not already used
        self.request_counter = self.request_counter + 1;
        self.request_counter
    }
}

pub enum FutureState {
    DataSaved(Result<String, std::io::Error>),
    DataSent(),
    BlockFetched(Block),
}

pub struct IoFuture {
    pub index: u64,
    future_wakers: Arc<Mutex<HashMap<u64, Waker>>>,
    future_states: Arc<Mutex<HashMap<u64, FutureState>>>,
}

pub struct RustIOHandler {
    sender: Sender<IoEvent>,
    handler_id: u8,
    future_index_counter: u64,
    future_wakers: Arc<Mutex<HashMap<u64, Waker>>>,
    future_states: Arc<Mutex<HashMap<u64, FutureState>>>,
}

impl RustIOHandler {
    pub fn new(sender: Sender<IoEvent>, handler_id: u8) -> RustIOHandler {
        RustIOHandler {
            sender,
            handler_id,
            future_index_counter: 0,
            future_wakers: Default::default(),
            future_states: Default::default(),
        }
    }

    pub fn process_interface_event(&mut self, event: InterfaceEvent) {
        todo!()
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
            .send(IoEvent {
                controller_id: 0,
                event: InterfaceEvent::OutgoingNetworkMessage {
                    peer_index,
                    message_name,
                    buffer,
                },
            })
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
            .send(IoEvent {
                controller_id: 0,
                event: InterfaceEvent::OutgoingNetworkMessageForAll {
                    message_name,
                    buffer,
                    exceptions: peer_exceptions,
                },
            })
            .await;
        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: Peer) -> Result<(), Error> {
        self.sender
            .send(IoEvent {
                controller_id: 0,
                event: InterfaceEvent::ConnectToPeer { peer_details: peer },
            })
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
    ) -> Result<(), Error> {
        debug!("writing value to disk : {:?}", key);
        let io_future = IoFuture {
            index: self.get_next_future_index(),
            future_wakers: self.future_wakers.clone(),
            future_states: self.future_states.clone(),
        };
        let result = self
            .sender
            .send(IoEvent {
                controller_id: 0,
                event: InterfaceEvent::DataSaveRequest {
                    key: result_key,
                    filename: key,
                    buffer: value,
                },
            })
            .await;

        if result.is_err() {
            warn!("{:?}", result.err().unwrap().to_string());
            return Err(Error::from(ErrorKind::Other));
        }
        Ok(())
        // TODO : DON't DELETE : can't wait on a future for results since this is keeping the blockchain lock with it.
        // debug!("waiting for future to complete");
        // let result = io_future.await;
        // if result.is_err() {
        //     debug!("failed writing value for disk");
        //     return Err(result.err().unwrap());
        // }
        // debug!("future is complete");
        // let result = result.unwrap();
        // match result {
        //     FutureState::DataSaved(result) => {
        //         return result;
        //     }
        //     _ => {
        //         unreachable!()
        //     }
        // }
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

    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

impl Future for IoFuture {
    type Output = Result<FutureState, std::io::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut future_states = self.future_states.lock().unwrap();
        let mut future_wakers = self.future_wakers.lock().unwrap();

        let result = future_states.remove(&self.index);
        if result.is_none() {
            future_wakers.insert(self.index, cx.waker().clone());
            return Poll::Pending;
        }
        Poll::Ready(Ok(result.unwrap()))
    }
}

use std::collections::HashMap;
use std::future::Future;
use std::io::Error;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use lazy_static::lazy_static;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};

use saito_core::common::command::InterfaceEvent;
use saito_core::common::handle_io::HandleIo;

use crate::IoEvent;

lazy_static! {
    static ref SHARED_CONTEXT: Mutex<IoContext> = Mutex::new(IoContext::new());
}

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

pub struct HandlerFuture {
    pub request_id: u64,
}

pub struct RustIOHandler {
    sender: Sender<IoEvent>,
    handler_id: u8,
}

impl RustIOHandler {
    pub fn new(sender: Sender<IoEvent>, handler_id: u8) -> RustIOHandler {
        RustIOHandler { sender, handler_id }
    }

    pub fn process_interface_event(&mut self, event: InterfaceEvent) {
        todo!()
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
        // let future = HandlerFuture { request_id: 0 };
        // future.await;
        todo!()
    }

    async fn process_interface_event(&mut self, event: InterfaceEvent) {
        todo!()
    }

    async fn write_value(&self, key: String, value: Vec<u8>) -> Result<(), Error> {
        todo!()
    }

    async fn read_value(&self, key: String) -> Result<Vec<u8>, Error> {
        todo!()
    }

    fn get_timestamp(&self) -> u64 {
        0
    }
}

impl Future for HandlerFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

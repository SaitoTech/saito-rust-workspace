use std::borrow::BorrowMut;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use log::debug;

use crate::common::handle_io::HandleIo;
use crate::common::run_task::RunTask;
use crate::core::blockchain::Blockchain;
use crate::core::context::Context;
use crate::core::mempool::Mempool;

pub struct Saito<T: HandleIo, S: RunTask> {
    pub io_handler: T,
    pub task_runner: S,
    pub context: Context,
}

impl<T: HandleIo, S: RunTask> Saito<T, S> {
    pub fn init(&mut self) {
        debug!("initializing saito core functionality");
        self.context.init(&self.task_runner);
    }
    pub fn process_message_buffer(&mut self, peer_index: u64, buffer: Vec<u8>) {}
    pub fn on_timer(&mut self, duration: Duration) -> Option<()> {
        None
    }
}

use std::sync::{Arc, RwLock};

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
    pub fn process_message_buffer(&mut self, peer_index: u64, buffer: Vec<u8>) {
        self.context
            .blockchain
            .read()
            .unwrap()
            .do_something(&self.task_runner);
    }
    pub fn on_timer(&mut self) -> Option<()> {
        todo!()
    }
}

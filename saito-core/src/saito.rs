// use std::borrow::BorrowMut;
// use std::sync::{Arc, RwLock};
// use std::time::Duration;
//
// use log::debug;
//
// use crate::common::handle_io::HandleIo;
// use crate::common::run_task::RunTask;
// use crate::core::test_data::blockchain::Blockchain;
// use crate::core::test_data::context::Context;
// use crate::core::test_data::mempool::Mempool;
//
// pub struct Saito<T: HandleIo, S: RunTask> {
//     pub io_handler: T,
//     pub task_runner: S,
//     pub context: Context,
// }
//
// impl<T: HandleIo, S: RunTask> Saito<T, S> {
//     pub fn init(&mut self) {
//         debug!("initializing saito core functionality");
//         self.context.init(&self.task_runner);
//     }
//
//     pub fn process_message_buffer(&mut self, peer_index: u64, buffer: Vec<u8>) {}
//
//     pub async fn on_timer(&mut self, duration: Duration) -> Option<()> {
//         let mut work_done = false;
//         let result = self.context.mempool.write().await.on_timer(duration);
//         work_done = work_done || result.is_some();
//         let result = self.context.miner.write().await.on_timer(duration);
//         work_done = work_done || result.is_some();
//         let result = self.context.blockchain.write().await.on_timer(duration);
//         work_done = work_done || result.is_some();
//
//         if work_done {
//             return Some(());
//         }
//         return None;
//     }
// }
//
// #[cfg(test)]
// mod test {}

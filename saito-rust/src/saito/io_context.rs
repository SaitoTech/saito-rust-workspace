// use std::collections::HashMap;
// use std::task::Waker;
//
// use crate::saito::rust_io_handler::FutureState;
//
// pub struct IoContext {
//     pub future_wakers: HashMap<u64, Waker>,
//     pub future_states: HashMap<u64, FutureState>,
// }
//
// impl IoContext {
//     pub fn new() -> IoContext {
//         IoContext {
//             future_wakers: Default::default(),
//             future_states: Default::default(),
//         }
//     }
// }

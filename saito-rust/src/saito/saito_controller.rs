use std::io::Error;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use log::info;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use saito_core::core::context::Context;
use saito_core::core::saito::Saito;

use crate::saito::command::Command;
use crate::saito::rust_io_handler::RustIOHandler;

pub struct SaitoController {
    pub saito: Saito<RustIOHandler>,
}

impl SaitoController {
    fn process_new_message(&mut self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        todo!()
    }
    fn on_timer(&mut self, duration: Duration) -> Option<()> {
        None
    }
}

pub async fn run_saito_controller(
    mut receiver: Receiver<Command>,
    mut sender_to_io_controller: Sender<Command>,
) {
    info!("running controller thread");
    let mut saito_controller = SaitoController {
        saito: Saito {
            io_handler: RustIOHandler {},
            context: Context::new(),
        },
    };

    let last_timestamp = Instant::now();
    let mut work_done = false;
    loop {
        work_done = false;

        let result = receiver.try_recv();
        if result.is_ok() {
            let command = result.unwrap();
            match command {
                Command::NetworkMessage(peer_index, buffer) => {
                    let result = saito_controller.process_new_message(peer_index, buffer);
                    work_done = true;
                }
                Command::DataSaveRequest(_, _) => {}
                Command::DataSaveResponse(_, _) => {}
                Command::DataReadRequest(_) => {}
                Command::DataReadResponse(_, _, _) => {}
            }
        }

        let current_instant = Instant::now();
        let duration = current_instant.duration_since(last_timestamp);
        let result = saito_controller.on_timer(duration);
        if result.is_some() {
            work_done = true;
        }

        if !work_done {
            std::thread::sleep(Duration::new(0, 1000));
        }
    }
}

use std::io::Error;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use log::info;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use saito_core::common::command::Command;
use saito_core::core::context::Context;
use saito_core::saito::Saito;

use crate::saito::rust_io_handler::RustIOHandler;
use crate::saito::rust_task_runner::RustTaskRunner;

pub struct SaitoController {
    pub saito: Saito<RustIOHandler, RustTaskRunner>,
}

impl SaitoController {
    fn process_network_message(&mut self, peer_index: u64, buffer: Vec<u8>) -> Result<(), Error> {
        info!("processing network message");

        self.saito.process_message_buffer(peer_index, buffer);
        Ok(())
    }
    fn on_timer(&mut self, duration: Duration) -> Option<()> {
        None
    }
}

pub async fn run_saito_controller(
    mut receiver: Receiver<Command>,
    mut sender_to_io_controller: Sender<Command>,
) {
    info!("running saito controller");
    let mut saito_controller = SaitoController {
        saito: Saito {
            io_handler: RustIOHandler {},
            task_runner: RustTaskRunner {},
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
                    info!("received network message");
                    let result = saito_controller.process_network_message(peer_index, buffer);
                    work_done = true;
                }
                Command::DataSaveRequest(_, _) => {
                    unreachable!()
                }
                Command::DataSaveResponse(_, _) => {}
                Command::DataReadRequest(_) => {
                    unreachable!()
                }
                Command::DataReadResponse(_, _, _) => {}
                Command::ConnectToPeer(_) => {
                    unreachable!()
                }
                Command::PeerConnected(_, _) => {}
                Command::PeerDisconnected(_) => {}
            }
        }

        let current_instant = Instant::now();
        let duration = current_instant.duration_since(last_timestamp);
        let result = saito_controller.on_timer(duration);
        if result.is_some() {
            work_done = true;
        }

        if !work_done {
            std::thread::sleep(Duration::new(1, 0));
        }
    }
}

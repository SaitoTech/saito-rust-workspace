use log::info;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

use saito_core::core::saito::Saito;

use crate::saito::command::Command;
use crate::saito::rust_io_handler::RustIOHandler;

pub struct Controller {
    pub saito: Saito<RustIOHandler>,
    pub receiver: Receiver<Command>,
}

impl Controller {}

pub async fn run_controller(receiver_in_controller: Receiver<Command>) {
    info!("running controller thread");
    loop {}
}

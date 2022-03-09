use log::info;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinHandle;

use saito_core::saito::Saito;

use crate::saito::command::Command;

pub struct Controller {
    pub saito: Saito,
    pub receiver: Receiver<Command>,
}

impl Controller {}

pub async fn run_controller(receiver_in_controller: Receiver<Command>) {
    info!("running controller thread");
    loop {}
}

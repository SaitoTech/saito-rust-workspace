use std::net::TcpListener;
use std::sync::mpsc::Receiver;

use log::info;
use tokio::task::JoinHandle;

use saito_core::saito::Saito;

use crate::saito::command::Command;

pub struct Controller {
    pub saito: Saito,
    pub receiver: Receiver<Command>,
}

impl Controller {}

pub async fn run_controller() {
    info!("running controller thread");
    loop {}
}

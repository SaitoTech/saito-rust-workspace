use std::net::TcpListener;
use std::sync::mpsc::Receiver;

use saito_core::saito::Saito;

use crate::saito::command::Command;

pub struct Controller {
    pub saito: Saito,
    pub receiver: Receiver<Command>,
    pub server: TcpListener,
}

impl Controller {
    fn run_server(&mut self) {
        todo!()
    }
}

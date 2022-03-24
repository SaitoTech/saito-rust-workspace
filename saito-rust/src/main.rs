use std::sync::Arc;

use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;

use saito_core::common::command::{GlobalEvent, InterfaceEvent};

use crate::saito::config_handler::ConfigHandler;
use crate::saito::io_controller::run_io_controller;
use crate::saito::io_event::IoEvent;
use crate::saito::saito_controller::run_saito_controller;

mod saito;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running saito");

    pretty_env_logger::init();
    let configs = Arc::new(RwLock::new(
        ConfigHandler::load_configs("configs/saito.config.json".to_string())
            .expect("loading configs failed"),
    ));

    let (sender_to_saito_controller, receiver_in_saito_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    let (sender_to_io_controller, receiver_in_io_controller) =
        tokio::sync::mpsc::channel::<IoEvent>(1000);

    let result1 = tokio::spawn(run_saito_controller(
        receiver_in_saito_controller,
        sender_to_io_controller.clone(),
        configs.clone(),
    ));

    let result2 = tokio::spawn(run_io_controller(
        receiver_in_io_controller,
        sender_to_saito_controller.clone(),
        configs.clone(),
    ));

    let result = tokio::join!(result1, result2);
    Ok(())
}

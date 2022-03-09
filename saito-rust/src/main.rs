use tokio::sync::mpsc::Receiver;

use crate::saito::command::Command;
use crate::saito::controller::run_controller;
use crate::saito::network_handler::run_network_handler;

mod saito;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running saito");

    pretty_env_logger::init();

    let (sender_to_controller, receiver_in_controller) =
        tokio::sync::mpsc::channel::<Command>(1000);

    let result1 = tokio::spawn(run_controller(receiver_in_controller));

    let result2 = tokio::spawn(run_network_handler(sender_to_controller.clone()));

    let result = tokio::join!(result1, result2);
    Ok(())
}

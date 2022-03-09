use crate::saito::controller::run_controller;
use crate::saito::network_handler::run_network_handler;

mod saito;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running saito");

    pretty_env_logger::init();

    let result1 = tokio::spawn(run_controller());

    let result2 = tokio::spawn(run_network_handler());

    let result = tokio::join!(result1, result2);
    Ok(())
}

use crate::saito::saito_node_app::SaitoNodeApp;
use log::error;
use std::panic;
use std::process;

mod saito;
mod test;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        if let Some(location) = panic_info.location() {
            error!(
                "panic occurred in file '{}' at line {}, exiting ..",
                location.file(),
                location.line()
            );
        } else {
            error!("panic occurred but can't get location information, exiting ..");
        }

        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(99);
    }));

    println!("Running saito");

    SaitoNodeApp::run("configs/saito.config.json".to_string()).await;

    Ok(())
}

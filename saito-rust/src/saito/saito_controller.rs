use std::future::Future;
use std::io::Error;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use log::info;
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use saito_core::common::command::{InterfaceEvent, SaitoEvent};
use saito_core::common::process_event::ProcessEvent;
use saito_core::common::run_task::RunnableTask;
use saito_core::core::blockchain_controller::{BlockchainController, BlockchainEvent};
use saito_core::core::data::context::Context;
use saito_core::core::mempool_controller::{MempoolController, MempoolEvent};
use saito_core::core::miner_controller::MinerEvent;
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
    async fn on_timer(&mut self, duration: Duration) -> Option<()> {
        self.saito.on_timer(duration).await
    }
}

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
    mut global_receiver: tokio::sync::broadcast::Receiver<SaitoEvent>,
    mut interface_event_receiver: Receiver<InterfaceEvent>,
    mut event_receiver: Receiver<T>,
) where
    T: Send + 'static,
{
    tokio::spawn(async move {
        let mut work_done = false;
        let mut last_timestamp = Instant::now();
        loop {
            let result = global_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_saito_event(event).is_some() {
                    work_done = true;
                }
            }

            let result = interface_event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_interface_event(event).is_some() {
                    work_done = true;
                }
            }

            let result = event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_event(event).is_some() {
                    work_done = true;
                }
            }

            let current_instant = Instant::now();
            let duration = current_instant.duration_since(last_timestamp);
            last_timestamp = current_instant;

            if event_processor.process_timer_event(duration).is_some() {
                work_done = true;
            }

            if work_done {
                work_done = false;
                std::thread::yield_now();
            } else {
                std::thread::sleep(Duration::new(0, 10_000));
            }
        }
    })
    .await;
}

pub async fn run_saito_controller(
    mut receiver: Receiver<InterfaceEvent>,
    mut sender_to_io_controller: Sender<InterfaceEvent>,
) {
    info!("running saito controller");

    let (global_sender, global_receiver) = tokio::sync::broadcast::channel::<SaitoEvent>(1000);

    let context = Context::new(global_sender.clone());

    let (sender_to_mempool, receiver_for_mempool) =
        tokio::sync::mpsc::channel::<MempoolEvent>(1000);
    let (sender_to_blockchain, receiver_for_blockchain) =
        tokio::sync::mpsc::channel::<BlockchainEvent>(1000);
    let (sender_to_miner, receiver_for_miner) = tokio::sync::mpsc::channel::<MinerEvent>(1000);

    let blockchain_controller = BlockchainController {
        blockchain: context.blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        sender_to_miner: sender_to_miner.clone(),
        io_handler: Box::new(RustIOHandler::new(sender_to_io_controller.clone())),
    };

    let mempool_controller = MempoolController {
        mempool: context.mempool.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_miner: sender_to_miner.clone(),
        io_handler: Box::new(RustIOHandler::new(sender_to_io_controller.clone())),
    };

    let (interface_sender_to_blockchain, interface_receiver_for_blockchain) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);

    let global_sender_clone = global_sender.clone();
    let blockchain_handle = run_thread(
        Box::new(blockchain_controller),
        global_sender_clone.subscribe(),
        interface_receiver_for_blockchain,
        receiver_for_blockchain,
    )
    .await;

    let mut saito_controller = SaitoController {
        saito: Saito {
            io_handler: RustIOHandler::new(sender_to_io_controller.clone()),
            task_runner: RustTaskRunner {},
            context: Context::new(global_sender.clone()),
        },
    };
    let global_receiver = global_sender.subscribe();
    saito_controller.saito.init();

    let mut last_timestamp = Instant::now();
    let mut work_done = false;
    loop {
        work_done = false;

        let result = receiver.try_recv();
        if result.is_ok() {
            let command = result.unwrap();
            match command {
                InterfaceEvent::IncomingNetworkMessage(peer_index, buffer) => {
                    info!("received network message");
                    let result = saito_controller.process_network_message(peer_index, buffer);
                    work_done = true;
                }
                InterfaceEvent::DataSaveRequest(_, _) => {
                    unreachable!()
                }
                InterfaceEvent::DataSaveResponse(_, _) => {}
                InterfaceEvent::DataReadRequest(_) => {
                    unreachable!()
                }
                InterfaceEvent::DataReadResponse(_, _, _) => {}
                InterfaceEvent::ConnectToPeer(_) => {
                    unreachable!()
                }
                InterfaceEvent::PeerConnected(_, _) => {}
                InterfaceEvent::PeerDisconnected(_) => {}
                _ => {}
            }
        }

        let current_instant = Instant::now();
        let duration = current_instant.duration_since(last_timestamp);
        last_timestamp = current_instant;
        let result = saito_controller.on_timer(duration).await;
        if result.is_some() {
            work_done = true;
        }

        if !work_done {
            std::thread::sleep(Duration::new(1, 0));
        }
    }
}

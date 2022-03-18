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

use saito_core::common::command::{GlobalEvent, InterfaceEvent};
use saito_core::common::process_event::ProcessEvent;
use saito_core::common::run_task::RunnableTask;
use saito_core::core::blockchain_controller::{BlockchainController, BlockchainEvent};
use saito_core::core::data::context::Context;
use saito_core::core::mempool_controller::{MempoolController, MempoolEvent};
use saito_core::core::miner_controller::{MinerController, MinerEvent};

use crate::saito::rust_io_handler::RustIOHandler;
use crate::saito::rust_task_runner::RustTaskRunner;
use crate::IoEvent;

pub struct SaitoController {}

impl SaitoController {}

async fn run_thread<T>(
    mut event_processor: Box<(dyn ProcessEvent<T> + Send + 'static)>,
    mut global_receiver: tokio::sync::broadcast::Receiver<GlobalEvent>,
    mut interface_event_receiver: Receiver<InterfaceEvent>,
    mut event_receiver: Receiver<T>,
) -> JoinHandle<()>
where
    T: Send + 'static,
{
    info!("starting new thread");
    tokio::spawn(async move {
        let mut work_done = false;
        let mut last_timestamp = Instant::now();
        loop {
            // TODO : refactor to support async calls
            let result = global_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_global_event(event).await.is_some() {
                    work_done = true;
                }
            }

            let result = interface_event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor
                    .process_interface_event(event)
                    .await
                    .is_some()
                {
                    work_done = true;
                }
            }

            let result = event_receiver.try_recv();
            if result.is_ok() {
                let event = result.unwrap();
                if event_processor.process_event(event).await.is_some() {
                    work_done = true;
                }
            }

            let current_instant = Instant::now();
            let duration = current_instant.duration_since(last_timestamp);
            last_timestamp = current_instant;

            if event_processor
                .process_timer_event(duration)
                .await
                .is_some()
            {
                work_done = true;
            }

            if work_done {
                work_done = false;
                std::thread::yield_now();
            } else {
                std::thread::sleep(Duration::new(0, 1000_000));
            }
        }
    })
}

pub async fn run_saito_controller(
    mut receiver: Receiver<IoEvent>,
    mut sender_to_io_controller: Sender<IoEvent>,
) {
    info!("running saito controller");

    const BLOCKCHAIN_CONTROLLER_ID: u8 = 1;
    const MEMPOOL_CONTROLLER_ID: u8 = 2;
    const MINER_CONTROLLER_ID: u8 = 3;

    let (global_sender, global_receiver) = tokio::sync::broadcast::channel::<GlobalEvent>(1000);

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
        io_handler: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            BLOCKCHAIN_CONTROLLER_ID,
        )),
    };
    let (interface_sender_to_blockchain, interface_receiver_for_blockchain) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);

    let blockchain_handle = run_thread(
        Box::new(blockchain_controller),
        global_sender.subscribe(),
        interface_receiver_for_blockchain,
        receiver_for_blockchain,
    )
    .await;

    let mempool_controller = MempoolController {
        mempool: context.mempool.clone(),
        blockchain: context.blockchain.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_miner: sender_to_miner.clone(),
        sender_global: global_sender.clone(),
        io_handler: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            MEMPOOL_CONTROLLER_ID,
        )),
    };
    let (interface_sender_to_mempool, interface_receiver_for_mempool) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);
    let mempool_handle = run_thread(
        Box::new(mempool_controller),
        global_sender.subscribe(),
        interface_receiver_for_mempool,
        receiver_for_mempool,
    )
    .await;

    let miner_controller = MinerController {
        miner: context.miner.clone(),
        sender_to_blockchain: sender_to_blockchain.clone(),
        sender_to_mempool: sender_to_mempool.clone(),
        io_handler: Box::new(RustIOHandler::new(
            sender_to_io_controller.clone(),
            MINER_CONTROLLER_ID,
        )),
    };
    let (interface_sender_to_miner, interface_receiver_for_miner) =
        tokio::sync::mpsc::channel::<InterfaceEvent>(1000);
    let miner_handle = run_thread(
        Box::new(miner_controller),
        global_sender.subscribe(),
        interface_receiver_for_miner,
        receiver_for_miner,
    )
    .await;

    // let mut saito_controller = SaitoController {
    //     saito: Saito {
    //         io_handler: RustIOHandler::new(sender_to_io_controller.clone()),
    //         task_runner: RustTaskRunner {},
    //         context: Context::new(global_sender.clone()),
    //     },
    // };
    // saito_controller.saito.init();

    let mut work_done = false;
    loop {
        work_done = false;

        let result = receiver.try_recv();
        if result.is_ok() {
            let command = result.unwrap();
            // TODO : remove hard coded values
            match command.controller_id {
                BLOCKCHAIN_CONTROLLER_ID => {
                    interface_sender_to_blockchain.send(command.event).await;
                }
                MEMPOOL_CONTROLLER_ID => {
                    interface_sender_to_mempool.send(command.event).await;
                }
                MINER_CONTROLLER_ID => {
                    interface_sender_to_miner.send(command.event).await;
                }
                _ => {
                    unreachable!()
                }
            }
        }

        if !work_done {
            std::thread::sleep(Duration::new(1, 0));
        }
    }
}

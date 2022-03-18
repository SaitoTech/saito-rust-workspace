use std::collections::VecDeque;
use std::io::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, info};
use tokio::sync::RwLock;

use crate::common::command::{GlobalEvent, InterfaceEvent};
use crate::common::defs::{SaitoPrivateKey, SaitoPublicKey};
use crate::common::process_event::ProcessEvent;
use crate::common::run_task::RunTask;
use crate::core::data::block::Block;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

pub struct Mempool {
    blocks_queue: VecDeque<Block>,
    pub transactions: Vec<Transaction>,
    // vector so we just copy it over
    routing_work_in_mempool: u64,
    wallet: Arc<RwLock<Wallet>>,
    currently_bundling_block: bool,
    public_key: SaitoPublicKey,
    private_key: SaitoPrivateKey,
}

impl Mempool {
    pub fn new(wallet: Arc<RwLock<Wallet>>) -> Mempool {
        Mempool {
            blocks_queue: Default::default(),
            transactions: vec![],
            routing_work_in_mempool: 0,
            wallet,
            currently_bundling_block: false,
            public_key: [0; 33],
            private_key: [0; 32],
        }
    }
    pub fn init(&mut self, task_runner: &dyn RunTask) -> Result<(), Error> {
        debug!("mempool.init");

        debug!("main thread id = {:?}", std::thread::current().id());
        task_runner.run(Box::pin(move || {
            let mut last_time = Instant::now();
            let mut counter = 0;
            debug!("new thread id = {:?}", std::thread::current().id());
            loop {
                let current_time = Instant::now();
                let duration = current_time.duration_since(last_time);

                if duration.as_micros() > 1_000_000 {
                    info!("counter : {:?}", counter);
                    last_time = current_time;
                    counter = counter + 1;
                }
                if counter < 5 {
                    continue;
                }
                info!("block created");

                counter = 0;
            }
        }));
        Ok(())
    }

    pub fn add_block(&mut self, block: Block) {
        todo!()
    }

    pub fn on_timer(&mut self, duration: Duration) -> Option<()> {
        None
    }
}

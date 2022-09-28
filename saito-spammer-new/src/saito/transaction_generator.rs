use crate::SpammerConfigs;
use std::cmp::min;
use std::collections::LinkedList;

use crate::saito::time_keeper::TimeKeeper;
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data::crypto::generate_random_bytes;
use saito_core::core::data::slip::{Slip, SLIP_SIZE};
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

use rayon::prelude::*;
use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, trace};

#[derive(Clone, PartialEq)]
pub enum GeneratorState {
    CreatingSlips,
    WaitingForBlockChainConfirmation,
    Done,
}

pub struct TransactionGenerator {
    state: GeneratorState,
    wallet: Arc<RwLock<Wallet>>,
    expected_slip_count: u64,
    tx_size: u32,
    tx_count: u64,
    time_keeper: Box<TimeKeeper>,
    public_key: SaitoPublicKey,
    private_key: SaitoPrivateKey,
    sender: Sender<Transaction>,
}

impl TransactionGenerator {
    pub async fn create(
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Box<SpammerConfigs>>>,
        sender: Sender<Transaction>,
    ) -> Self {
        let tx_size = 10;
        let tx_count;
        {
            let config = configuration.read().await;
            // tx_size = config.spammer.tx_size;
            tx_count = config.get_spammer_configs().tx_count;
        }

        let mut res = TransactionGenerator {
            state: GeneratorState::CreatingSlips,
            wallet: wallet.clone(),
            expected_slip_count: 1,
            tx_size,
            tx_count,
            time_keeper: Box::new(TimeKeeper {}),
            public_key: [0; 33],
            private_key: [0; 32],
            sender,
        };
        {
            log_read_lock_request!("wallet");
            let wallet = wallet.read().await;
            log_read_lock_receive!("wallet");
            res.public_key = wallet.public_key;
            res.private_key = wallet.private_key;
        }
        res
    }

    pub fn get_state(&self) -> GeneratorState {
        return self.state.clone();
    }
    pub async fn on_new_block(&mut self) -> bool {
        match self.state {
            GeneratorState::CreatingSlips => {
                return self.create_slips().await;
            }
            GeneratorState::WaitingForBlockChainConfirmation => {
                if self.check_blockchain_for_confirmation().await {
                    self.create_test_transactions().await;
                    self.state = GeneratorState::Done;
                    return true;
                }
            }
            GeneratorState::Done => {}
        }
        return false;
    }

    async fn create_slips(&mut self) -> bool {
        let output_slips_per_input_slip: u8 = 100;
        let unspent_slip_count;
        let available_balance;

        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet.read().await;
            log_read_lock_receive!("wallet");

            unspent_slip_count = wallet.get_unspent_slip_count();
            available_balance = wallet.get_available_balance();
        }

        if unspent_slip_count < self.tx_count && unspent_slip_count >= self.expected_slip_count {
            info!(
                "Creating new slips, current = {:?}, target = {:?} balance = {:?}",
                unspent_slip_count, self.tx_count, available_balance
            );

            let total_nolans_requested_per_slip = available_balance / unspent_slip_count;
            let mut total_output_slips_created: u64 = 0;

            for _i in 0..unspent_slip_count {
                let transaction = self
                    .create_slip_transaction(
                        output_slips_per_input_slip,
                        total_nolans_requested_per_slip,
                        &mut total_output_slips_created,
                    )
                    .await;

                // transactions.push_back(transaction);
                self.sender.send(transaction).await.unwrap();

                if total_output_slips_created >= self.tx_count {
                    info!(
                        "Slip creation completed, current = {:?}, target = {:?}",
                        total_output_slips_created, self.tx_count
                    );
                    self.state = GeneratorState::WaitingForBlockChainConfirmation;
                    break;
                }
            }

            self.expected_slip_count = total_output_slips_created;

            info!(
                "New slips created, current = {:?}, target = {:?}",
                total_output_slips_created, self.tx_count
            );
            return true;
        }
        false
    }

    async fn create_slip_transaction(
        &mut self,
        output_slips_per_input_slip: u8,
        total_nolans_requested_per_slip: u64,
        total_output_slips_created: &mut u64,
    ) -> Transaction {
        let payment_amount = total_nolans_requested_per_slip / output_slips_per_input_slip as u64;

        log_write_lock_request!("wallet");
        let mut wallet = self.wallet.write().await;
        log_write_lock_receive!("wallet");

        let mut transaction = Transaction::new();

        let (mut input_slips, mut output_slips) =
            wallet.generate_slips(total_nolans_requested_per_slip);

        let input_len = input_slips.len();
        let output_len = output_slips.len();

        for _a in 0..input_len {
            transaction.add_input(input_slips[0].clone());
            input_slips.remove(0);
        }

        for _b in 0..output_len {
            transaction.add_output(output_slips[0].clone());
            output_slips.remove(0);
        }

        for _c in 0..output_slips_per_input_slip {
            let mut output = Slip::new();
            output.public_key = self.public_key;
            output.amount = payment_amount;
            transaction.add_output(output);
            *total_output_slips_created += 1;
        }

        let remaining_bytes: i64 =
            self.tx_size as i64 - (*total_output_slips_created + 1) as i64 * SLIP_SIZE as i64;

        if remaining_bytes > 0 {
            transaction.message = generate_random_bytes(remaining_bytes as u64);
        }

        transaction.timestamp = self.time_keeper.get_timestamp();
        transaction.generate(self.public_key, 0, 0);
        transaction.sign(self.private_key);
        transaction.add_hop(&wallet, self.public_key);

        return transaction;
    }

    async fn check_blockchain_for_confirmation(&mut self) -> bool {
        let unspent_slip_count;
        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet.read().await;
            log_read_lock_receive!("wallet");
            unspent_slip_count = wallet.get_unspent_slip_count();
        }

        if unspent_slip_count >= self.tx_count {
            info!(
                "New slips detected on the blockchain, current = {:?}, target = {:?}",
                unspent_slip_count, self.tx_count
            );
            self.state = GeneratorState::Done;
            return true;
        }
        debug!(
            "unspent slips : {:?} tx count : {:?}",
            unspent_slip_count, self.tx_count
        );
        return false;
    }

    async fn create_test_transactions(&mut self) -> JoinHandle<()> {
        info!("creating test transactions : {:?}", self.tx_count);

        let sender = self.sender.clone();
        let wallet = self.wallet.clone();
        let public_key = self.public_key.clone();
        let private_key = self.private_key.clone();
        let tx_count = self.tx_count.clone();
        let tx_size = self.tx_size.clone();
        return tokio::spawn(async move {
            let time_keeper = TimeKeeper {};

            let mut required_count = tx_count;

            loop {
                {
                    log_write_lock_request!("wallet");
                    let mut wallet = wallet.write().await;
                    log_write_lock_receive!("wallet");
                    let create_count = min(1000, required_count);
                    for _i in 0..create_count {
                        let mut transaction = Transaction::create(&mut wallet, public_key, 1, 0);
                        transaction.message = generate_random_bytes(tx_size as u64);
                        transaction.timestamp = time_keeper.get_timestamp();
                        transaction.generate(public_key, 0, 0);
                        transaction.sign(private_key);
                        transaction.add_hop(&wallet, public_key);

                        sender.send(transaction).await.unwrap();

                        required_count -= 1;
                    }
                }
                info!("test transactions to be created : {:?}", required_count);
                // tokio::time::sleep(Duration::from_millis(10)).await;
                if required_count == 0 {
                    break;
                }
            }
            info!("Test transactions created, count : {:?}", tx_count);
        });
    }
}

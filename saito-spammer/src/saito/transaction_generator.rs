use crate::SpammerConfiguration;

use crate::saito::time_keeper::TimeKeeper;
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data::crypto::generate_random_bytes;
use saito_core::core::data::slip::{Slip, SLIP_SIZE};
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};
use std::collections::LinkedList;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Clone)]
pub enum GeneratorState {
    CreatingSlips,
    WaitingForBlockChainConfirmation,
    Ready,
}

pub struct TransactionGenerator {
    state: GeneratorState,
    wallet: Arc<RwLock<Wallet>>,
    expected_slip_count: u64,
    tx_size: u32,
    tx_count: u64,
    time_keeper: Box<TimeKeeper>,
}

impl TransactionGenerator {
    pub async fn create(
        wallet: Arc<RwLock<Wallet>>,
        configuration: Arc<RwLock<Box<SpammerConfiguration>>>,
    ) -> Self {
        let tx_size;
        let tx_count;
        {
            let config = configuration.read().await;
            tx_size = config.get_spammer_configs().tx_size;
            tx_count = config.get_spammer_configs().tx_count;
        }

        return TransactionGenerator {
            state: GeneratorState::CreatingSlips,
            wallet,
            expected_slip_count: 1,
            tx_size,
            tx_count,
            time_keeper: Box::new(TimeKeeper {}),
        };
    }

    pub async fn on_new_block(&mut self) -> Option<LinkedList<Transaction>> {
        match self.state {
            GeneratorState::CreatingSlips => {
                return self.create_slips().await;
            }
            GeneratorState::WaitingForBlockChainConfirmation => {
                if self.check_blockchain_for_confirmation().await {
                    return self.create_test_transactions().await;
                }
            }
            GeneratorState::Ready => {}
        }

        return None;
    }

    async fn create_slips(&mut self) -> Option<LinkedList<Transaction>> {
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
                "Creating new slips, current = {:?}, target = {:?}",
                unspent_slip_count, self.tx_count
            );

            let total_nolans_requested_per_slip = available_balance / unspent_slip_count;
            let mut total_output_slips_created: u64 = 0;
            let mut transactions: LinkedList<Transaction> = Default::default();

            for _i in 0..unspent_slip_count {
                let transaction = self
                    .create_slip_transaction(
                        output_slips_per_input_slip,
                        total_nolans_requested_per_slip,
                        &mut total_output_slips_created,
                        &TimeKeeper {},
                    )
                    .await;

                transactions.push_back(transaction);

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

            return Some(transactions);
        }

        return None;
    }

    async fn create_slip_transaction(
        &mut self,
        output_slips_per_input_slip: u8,
        total_nolans_requested_per_slip: u64,
        total_output_slips_created: &mut u64,
        time_keeper: &TimeKeeper,
    ) -> Transaction {
        let payment_amount = total_nolans_requested_per_slip / output_slips_per_input_slip as u64;

        let mut transaction;
        let public_key;
        let private_key;
        {
            log_write_lock_request!("wallet");
            let mut wallet = self.wallet.write().await;
            log_write_lock_receive!("wallet");

            public_key = wallet.public_key;
            private_key = wallet.private_key;
            transaction = Transaction::new();

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
        }

        for _c in 0..output_slips_per_input_slip {
            let mut output = Slip::new();
            output.public_key = public_key;
            output.amount = payment_amount;
            transaction.add_output(output);
            *total_output_slips_created += 1;
        }

        let remaining_bytes: i64 =
            self.tx_size as i64 - (*total_output_slips_created + 1) as i64 * SLIP_SIZE as i64;

        if remaining_bytes > 0 {
            transaction.message = generate_random_bytes(remaining_bytes as u64);
        }

        transaction.timestamp = time_keeper.get_timestamp();
        transaction.generate(public_key);
        transaction.sign(private_key);
        transaction.add_hop(self.wallet.clone(), public_key).await;

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
            self.state = GeneratorState::Ready;
            return true;
        }

        return false;
    }

    async fn create_test_transactions(&mut self) -> Option<LinkedList<Transaction>> {
        let public_key;
        let private_key;
        {
            log_read_lock_request!("wallet");
            let wallet = self.wallet.read().await;
            log_read_lock_receive!("wallet");
            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let mut transactions: LinkedList<Transaction> = Default::default();

        for _i in 0..self.tx_count {
            let mut transaction = Transaction::create(self.wallet.clone(), public_key, 1, 1).await;
            transaction.message = generate_random_bytes(self.tx_size as u64);
            transaction.generate(public_key);
            transaction.sign(private_key);
            transaction.add_hop(self.wallet.clone(), public_key).await;

            transactions.push_back(transaction);
        }

        info!(
            "Test transactions created, count = {:?}",
            transactions.len()
        );

        return Some(transactions);
    }
}

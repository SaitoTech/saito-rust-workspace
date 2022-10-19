use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use rayon::prelude::*;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tracing::{debug, info};

use saito_core::common::defs::{Currency, SaitoPrivateKey, SaitoPublicKey};
use saito_core::common::keep_time::KeepTime;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::crypto::generate_random_bytes;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::slip::{Slip, SLIP_SIZE};
use saito_core::core::data::transaction::Transaction;
use saito_core::core::data::wallet::Wallet;
use saito_core::{
    log_read_lock_receive, log_read_lock_request, log_write_lock_receive, log_write_lock_request,
};

use crate::saito::time_keeper::TimeKeeper;
use crate::SpammerConfigs;

#[derive(Clone, PartialEq)]
pub enum GeneratorState {
    CreatingSlips,
    WaitingForBlockChainConfirmation,
    Done,
}

pub struct TransactionGenerator {
    state: GeneratorState,
    wallet: Arc<RwLock<Wallet>>,
    blockchain: Arc<RwLock<Blockchain>>,
    expected_slip_count: u64,
    tx_size: u32,
    tx_count: u64,
    time_keeper: Box<TimeKeeper>,
    public_key: SaitoPublicKey,
    private_key: SaitoPrivateKey,
    sender: Sender<VecDeque<Transaction>>,
    tx_payment: Currency,
    tx_fee: Currency,
    peers: Arc<RwLock<PeerCollection>>,
}

impl TransactionGenerator {
    pub async fn create(
        wallet: Arc<RwLock<Wallet>>,
        peers: Arc<RwLock<PeerCollection>>,
        blockchain: Arc<RwLock<Blockchain>>,
        configuration: Arc<RwLock<Box<SpammerConfigs>>>,
        sender: Sender<VecDeque<Transaction>>,
        tx_payment: Currency,
        tx_fee: Currency,
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
            blockchain,
            expected_slip_count: 1,
            tx_size,
            tx_count,
            time_keeper: Box::new(TimeKeeper {}),
            public_key: [0; 33],
            private_key: [0; 32],
            sender,
            tx_payment,
            tx_fee,
            peers,
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
    pub async fn on_new_block(&mut self) {
        match self.state {
            GeneratorState::CreatingSlips => {
                self.create_slips().await;
            }
            GeneratorState::WaitingForBlockChainConfirmation => {
                if self.check_blockchain_for_confirmation().await {
                    self.create_test_transactions().await;
                    self.state = GeneratorState::Done;
                }
            }
            GeneratorState::Done => {}
        }
    }

    async fn create_slips(&mut self) {
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

            let total_nolans_requested_per_slip =
                available_balance / unspent_slip_count as Currency;
            let mut total_output_slips_created: u64 = 0;

            let mut to_public_key = [0; 33];

            {
                log_read_lock_request!("peers");
                let peers = self.peers.read().await;
                log_read_lock_receive!("peers");
                for peer in peers.index_to_peers.iter() {
                    to_public_key = peer.1.public_key.clone().unwrap();
                    break;
                }
                assert_eq!(peers.address_to_peers.len(), 1 as usize, "we have assumed connecting to a single node. move add_hop to correct place if not.");
                assert_ne!(to_public_key, self.public_key);
            }
            let mut txs: VecDeque<Transaction> = Default::default();
            for _i in 0..unspent_slip_count {
                let transaction = self
                    .create_slip_transaction(
                        output_slips_per_input_slip,
                        total_nolans_requested_per_slip,
                        &mut total_output_slips_created,
                        &to_public_key,
                    )
                    .await;

                // txs.push_back(transaction);
                txs.push_back(transaction);

                if total_output_slips_created >= self.tx_count {
                    info!(
                        "Slip creation completed, current = {:?}, target = {:?}",
                        total_output_slips_created, self.tx_count
                    );
                    self.state = GeneratorState::WaitingForBlockChainConfirmation;
                    break;
                }
            }
            self.sender.send(txs).await.unwrap();

            self.expected_slip_count = total_output_slips_created;

            info!(
                "New slips created, current = {:?}, target = {:?}",
                total_output_slips_created, self.tx_count
            );
        }
    }

    async fn create_slip_transaction(
        &mut self,
        output_slips_per_input_slip: u8,
        total_nolans_requested_per_slip: Currency,
        total_output_slips_created: &mut u64,
        to_public_key: &SaitoPublicKey,
    ) -> Transaction {
        let payment_amount =
            total_nolans_requested_per_slip / output_slips_per_input_slip as Currency;

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
        transaction.generate(&self.public_key, 0, 0);
        transaction.sign(&self.private_key);
        transaction.add_hop(&wallet.private_key, &wallet.public_key, to_public_key);

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

    async fn create_test_transactions(&mut self) {
        info!("creating test transactions : {:?}", self.tx_count);

        let time_keeper = TimeKeeper {};
        let wallet = self.wallet.clone();
        let blockchain = self.blockchain.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1000);
        let public_key = self.public_key.clone();
        let count = 1000000;
        let required_balance = (self.tx_payment + self.tx_fee) * count as Currency;
        let payment = self.tx_payment;
        let fee = self.tx_fee;
        tokio::spawn(async move {
            let sender = sender.clone();
            loop {
                let mut work_done = false;
                {
                    log_write_lock_request!("blockchain");
                    let mut blockchain = blockchain.write().await;
                    log_write_lock_receive!("blockchain");
                    log_write_lock_request!("wallet");
                    let mut wallet = wallet.write().await;
                    log_write_lock_receive!("wallet");
                    if wallet.get_available_balance() >= required_balance {
                        assert_ne!(blockchain.utxoset.len(), 0);
                        let mut vec = VecDeque::with_capacity(count);
                        for _ in 0..count {
                            let transaction =
                                Transaction::create(&mut wallet, public_key, payment, fee);
                            vec.push_back(transaction);
                        }
                        sender.send(vec).await.unwrap();
                        work_done = true;
                    }
                }
                if !work_done {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });

        let mut to_public_key = [0; 33];

        {
            log_read_lock_request!("peers");
            let peers = self.peers.read().await;
            log_read_lock_receive!("peers");
            for peer in peers.index_to_peers.iter() {
                to_public_key = peer.1.public_key.clone().unwrap();
                break;
            }
            assert_eq!(peers.address_to_peers.len(), 1 as usize, "we have assumed connecting to a single node. move add_hop to correct place if not.");
            assert_ne!(to_public_key, self.public_key);
        }

        while let Some(mut transactions) = receiver.recv().await {
            let sender = self.sender.clone();
            let tx_size = self.tx_size;

            let txs: VecDeque<Transaction> = transactions
                .par_drain(..)
                .with_min_len(100)
                .map(|mut transaction| {
                    transaction.message = vec![0; tx_size as usize]; //;generate_random_bytes(tx_size as u64);
                    transaction.timestamp = time_keeper.get_timestamp();
                    transaction.generate(&public_key, 0, 0);
                    transaction.sign(&self.private_key);
                    transaction.add_hop(&self.private_key, &self.public_key, &to_public_key);

                    transaction
                    // sender.send(transaction).await.unwrap();
                })
                .collect();
            sender.send(txs).await.unwrap();
        }

        // info!("Test transactions created, count : {:?}", txs.len());
    }
}

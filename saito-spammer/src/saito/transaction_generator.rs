use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use log::{debug, info, trace};
use rayon::prelude::*;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use saito_core::core::consensus::blockchain::Blockchain;
use saito_core::core::consensus::peer_collection::PeerCollection;
use saito_core::core::consensus::slip::{Slip, SLIP_SIZE};
use saito_core::core::consensus::transaction::Transaction;
use saito_core::core::consensus::wallet::Wallet;
use saito_core::core::defs::{
    push_lock, Currency, SaitoPrivateKey, SaitoPublicKey, LOCK_ORDER_BLOCKCHAIN,
    LOCK_ORDER_CONFIGS, LOCK_ORDER_PEERS, LOCK_ORDER_WALLET,
};
use saito_core::core::process::keep_time::KeepTime;
use saito_core::core::util::crypto::generate_random_bytes;
use saito_core::{drain, lock_for_read, lock_for_write};
use saito_rust::saito::time_keeper::TimeKeeper;

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
    tx_size: u64,
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
        wallet_lock: Arc<RwLock<Wallet>>,
        peers_lock: Arc<RwLock<PeerCollection>>,
        blockchain_lock: Arc<RwLock<Blockchain>>,
        configuration_lock: Arc<RwLock<SpammerConfigs>>,
        sender: Sender<VecDeque<Transaction>>,
        tx_payment: Currency,
        tx_fee: Currency,
    ) -> Self {
        let mut tx_size = 10;
        let tx_count;
        {
            let (configs, _configs_) = lock_for_read!(configuration_lock, LOCK_ORDER_CONFIGS);

            tx_size = configs.get_spammer_configs().tx_size;
            tx_count = configs.get_spammer_configs().tx_count;
        }

        let mut res = TransactionGenerator {
            state: GeneratorState::CreatingSlips,
            wallet: wallet_lock.clone(),
            blockchain: blockchain_lock.clone(),
            expected_slip_count: 1,
            tx_size,
            tx_count,
            time_keeper: Box::new(TimeKeeper {}),
            public_key: [0; 33],
            private_key: [0; 32],
            sender,
            tx_payment,
            tx_fee,
            peers: peers_lock.clone(),
        };
        {
            let (wallet, _wallet_) = lock_for_read!(wallet_lock, LOCK_ORDER_WALLET);
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
        trace!("creating slips for spammer");
        let output_slips_per_input_slip: u8 = 100;
        let unspent_slip_count;
        let available_balance;

        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);

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
                let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

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
        } else {
            trace!(
                "not enough slips. unspent slip count : {:?} tx count : {:?} expected slips : {:?}",
                unspent_slip_count,
                self.tx_count,
                self.expected_slip_count
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

        let (mut wallet, _wallet_) = lock_for_write!(self.wallet, LOCK_ORDER_WALLET);

        let mut transaction = Transaction::default();

        let (input_slips, output_slips) =
            wallet.generate_slips(total_nolans_requested_per_slip, None);

        for slip in input_slips {
            transaction.add_from_slip(slip);
        }
        for slip in output_slips {
            transaction.add_to_slip(slip);
        }

        for _c in 0..output_slips_per_input_slip {
            let mut output = Slip::default();
            output.public_key = self.public_key;
            output.amount = payment_amount;
            transaction.add_to_slip(output);
            *total_output_slips_created += 1;
        }

        let remaining_bytes: i64 =
            self.tx_size as i64 - (*total_output_slips_created + 1) as i64 * SLIP_SIZE as i64;

        if remaining_bytes > 0 {
            transaction.data = generate_random_bytes(remaining_bytes as u64);
        }

        transaction.timestamp = self.time_keeper.get_timestamp_in_ms();
        transaction.generate(&self.public_key, 0, 0);
        transaction.sign(&self.private_key);
        transaction.add_hop(&wallet.private_key, &wallet.public_key, to_public_key);

        return transaction;
    }

    async fn check_blockchain_for_confirmation(&mut self) -> bool {
        let unspent_slip_count;
        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet, LOCK_ORDER_WALLET);
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
        trace!(
            "unspent slips : {:?} tx count : {:?}",
            unspent_slip_count,
            self.tx_count
        );
        return false;
    }

    async fn create_test_transactions(&mut self) {
        info!("creating test transactions : {:?}", self.tx_count);

        let time_keeper = TimeKeeper {};
        let wallet = self.wallet.clone();
        let blockchain = self.blockchain.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(10);
        let public_key = self.public_key.clone();
        let count = self.tx_count;
        let required_balance = (self.tx_payment + self.tx_fee) * count as Currency;
        let payment = self.tx_payment;
        let fee = self.tx_fee;
        tokio::spawn(async move {
            let sender = sender.clone();
            loop {
                let mut work_done = false;
                {
                    let (blockchain, _blockchain_) =
                        lock_for_write!(blockchain, LOCK_ORDER_BLOCKCHAIN);

                    let (mut wallet, _wallet_) = lock_for_write!(wallet, LOCK_ORDER_WALLET);

                    if wallet.get_available_balance() >= required_balance {
                        assert_ne!(blockchain.utxoset.len(), 0);
                        let mut vec = VecDeque::with_capacity(count as usize);
                        for _ in 0..count {
                            let transaction = Transaction::create(
                                &mut wallet,
                                public_key,
                                payment,
                                fee,
                                false,
                                None,
                            );
                            if transaction.is_err() {
                                break;
                            }
                            let mut transaction = transaction.unwrap();
                            transaction.generate_total_fees(0, 0);
                            if (transaction.total_in == 0 || transaction.total_out == 0)
                                && (payment + fee != 0)
                            {
                                debug!("transaction not added since not enough funds. in : {:?} out : {:?}. current balance : {:?}, required : {:?}", transaction.total_in, transaction.total_out,wallet.get_available_balance(), required_balance);
                                break;
                            }
                            vec.push_back(transaction);
                        }
                        if !vec.is_empty() {
                            sender.send(vec).await.unwrap();
                            work_done = true;
                        }
                    }
                }
                if !work_done {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });

        let mut to_public_key = [0; 33];

        {
            let (peers, _peers_) = lock_for_read!(self.peers, LOCK_ORDER_PEERS);

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

            let txs: VecDeque<Transaction> = drain!(transactions, 100)
                .map(|mut transaction| {
                    transaction.data = vec![0; tx_size as usize]; //;generate_random_bytes(tx_size as u64);
                    transaction.timestamp = time_keeper.get_timestamp_in_ms();
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

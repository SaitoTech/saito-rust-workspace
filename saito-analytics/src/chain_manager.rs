use std::borrow::BorrowMut;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ahash::AHashMap;
use log::{debug, info};
use std::error::Error;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::Write;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;

use crate::test_io_handler::TestIOHandler;
use saito_core::common::defs::{
    push_lock, Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, Timestamp,
    UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET,
};

use saito_core::core::data::block::Block;
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::configuration::{Configuration, PeerConfig, Server};
use saito_core::core::data::crypto::{
    generate_keys, generate_random_bytes, hash, verify_signature,
};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_thread::MiningEvent;
use saito_core::{lock_for_read, lock_for_write};

struct TestConfiguration {}

impl Debug for TestConfiguration {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Configuration for TestConfiguration {
    fn get_server_configs(&self) -> Option<&Server> {
        todo!()
    }

    fn get_peer_configs(&self) -> &Vec<PeerConfig> {
        todo!()
    }

    fn get_block_fetch_url(&self) -> String {
        todo!()
    }

    fn is_spv_mode(&self) -> bool {
        false
    }

    fn is_browser(&self) -> bool {
        false
    }

    fn replace(&mut self, _config: &dyn Configuration) {
        todo!()
    }
}

pub fn create_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as Timestamp
}

pub struct ChainManager {
    pub mempool_lock: Arc<RwLock<Mempool>>,
    pub blockchain_lock: Arc<RwLock<Blockchain>>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub latest_block_hash: SaitoHash,
    pub network: Network,
    pub storage: Storage,
    pub peers: Arc<RwLock<PeerCollection>>,
    pub sender_to_miner: Sender<MiningEvent>,
    pub receiver_in_miner: Receiver<MiningEvent>,
    pub configs: Arc<RwLock<dyn Configuration + Send + Sync>>,
}

impl ChainManager {
    pub fn new() -> Self {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);
        let _public_key = wallet.public_key.clone();
        let _private_key = wallet.private_key.clone();
        let peers = Arc::new(RwLock::new(PeerCollection::new()));
        let wallet_lock = Arc::new(RwLock::new(wallet));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let mempool_lock = Arc::new(RwLock::new(Mempool::new(wallet_lock.clone())));
        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(10);
        let configs = Arc::new(RwLock::new(TestConfiguration {}));

        Self {
            wallet_lock: wallet_lock.clone(),
            blockchain_lock,
            mempool_lock,
            latest_block_hash: [0; 32],
            network: Network::new(
                Box::new(TestIOHandler::new()),
                peers.clone(),
                wallet_lock.clone(),
                configs.clone(),
            ),
            peers: peers.clone(),
            storage: Storage::new(Box::new(TestIOHandler::new())),
            sender_to_miner: sender_to_miner.clone(),
            receiver_in_miner,
            configs,
        }
    }

    pub fn show_info(&self) {
        println!("info about ...");
    }

    pub fn get_mempool_lock(&self) -> Arc<RwLock<Mempool>> {
        return self.mempool_lock.clone();
    }

    pub fn get_wallet_lock(&self) -> Arc<RwLock<Wallet>> {
        return self.wallet_lock.clone();
    }

    pub fn get_blockchain_lock(&self) -> Arc<RwLock<Blockchain>> {
        return self.blockchain_lock.clone();
    }

    pub async fn wait_for_mining_event(&mut self) {
        self.receiver_in_miner
            .recv()
            .await
            .expect("mining event receive failed");
    }

    //
    // add block to blockchain
    //
    pub async fn add_block(&mut self, block: Block) {
        debug!("adding block to test manager blockchain");
        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        let (mut mempool, _mempool_) = lock_for_write!(self.mempool_lock, LOCK_ORDER_MEMPOOL);

        blockchain
            .add_block(
                block,
                &mut self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
                &mut mempool,
                configs.deref(),
            )
            .await;
        debug!("block added to test manager blockchain");
    }

    //
    // check that the blockchain connects properly
    //
    pub async fn check_blockchain(&self) {
        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        for i in 1..blockchain.blocks.len() {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);

            let previous_block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id((i as u64) - 1);

            let block = blockchain.get_block_sync(&block_hash);
            let previous_block = blockchain.get_block_sync(&previous_block_hash);

            if block_hash == [0; 32] {
                assert_eq!(block.is_none(), true);
            } else {
                assert_eq!(block.is_none(), false);
                if i != 1 && previous_block_hash != [0; 32] {
                    assert_eq!(previous_block.is_none(), false);
                    assert_eq!(
                        block.unwrap().previous_block_hash,
                        previous_block.unwrap().hash
                    );
                }
            }
        }
    }

    //delete
    pub async fn dump_utxoset_tmp(&self, threshold: i32) -> Result<(), Box<dyn Error>> {
        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        let mut file = File::create("data/utxoset.txt")?; // Use await and ? here

        // print to file in a format similar to the issuance files
        //publickey \t amount \t type (normal)
        //for entries in the hashmap that are spendable (i.e. key => value (1) )

        let latest_block_id = blockchain.get_latest_block_id();
        println!("latest_block_id {}", latest_block_id);

        for i in 1..=latest_block_id {
            println!("check {}", i);
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            println!("WINDING ID HASH - {} {:?}", i, block_hash);
            let block = blockchain.get_block(&block_hash).unwrap();
            for j in 0..block.transactions.len() {
                println!("tx j: {}", j);

                println!("from len {:?}", block.transactions[j].from.len());
                println!("to len {:?}", block.transactions[j].to.len());

                block.transactions[j].from.iter().for_each(|input| {
                    //input.on_chain_reorganization(utxoset, longest_chain, input_slip_spendable)
                });

                //block.transactions[j].on_chain_reorganization(&blockchain.utxoset, true, i as u64);
            }
        }

        for (key, value) in &blockchain.utxoset {
            let key_hex = hex::encode(&(*key));
            if *value {
                //spendable only
                writeln!(file, "{}: {}", key_hex, value)?;
            }
        }

        //self.from.iter().for_each(|input| {
        //     input.on_chain_reorganization(utxoset, longest_chain, input_slip_spendable)
        // });
        // self.to.iter().for_each(|output| {
        //     output.on_chain_reorganization(utxoset, longest_chain, output_slip_spendable)
        // });

        Ok(())
    }

    //
    // check that everything spendable in the main UTXOSET is spendable on the longest
    // chain and vice-versa.
    //
    pub async fn dump_utxoset(&self, threshold: u64) {
        //info!("check_utxoset");
        println!("check_utxoset");
        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        type UtxoSetBalance = AHashMap<String, u64>;

        //let mut utxoset: UtxoSet = AHashMap::new();
        let latest_block_id = blockchain.get_latest_block_id();

        let mut utxo_balances: UtxoSetBalance = AHashMap::new();

        //let mut file = File::create("data/utxoset.txt"); // Use await and ? here
        let mut file = File::create("data/utxoset.txt").unwrap();

        println!("UTXO state height: latest_block_id {}", latest_block_id);
        writeln!(
            file,
            "UTXO state height: latest_block_id {}",
            latest_block_id
        );

        for i in 1..=latest_block_id {
            //println!("check {}", i);
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            //println!("WINDING ID HASH - {} {:?}", i, block_hash);
            let block = blockchain.get_block(&block_hash).unwrap();
            for j in 0..block.transactions.len() {
                let mut tx = &block.transactions[j];
                //println!("j: {}", j);
                //println!("tx: {:?}", tx);
                //UPDATE
                //tx.on_chain_reorganization(&mut utxoset, true, i as u64);
                //utxoset.insert(self.utxoset_key, spendable);
                //println!(">>> {}", utxoset.len());
                let input_slip_spendable = false;
                let output_slip_spendable = true;

                // println!("tx from >> {}", tx.from.len());
                // println!("tx to >> {}", tx.to.len());
                // println!("transaction_type  >> {:?}", tx.transaction_type);

                tx.from.iter().for_each(|input| {
                    //input.on_chain_reorganization(&utxoset, longest_chain, input_slip_spendable)
                    //println!("from {:?}", input);
                    let input_hex = hex::encode(input.public_key);
                    let balance = utxo_balances.entry(input_hex).or_insert(0);
                    *balance -= input.amount;
                });

                //publickey \t amount \t type (normal)

                //TODO check spendable only

                tx.to.iter().for_each(|output| {
                    //input.on_chain_reorganization(&utxoset, longest_chain, input_slip_spendable)
                    let output_hex = hex::encode(output.public_key);
                    //println!(">> {:?}", output_hex);
                    //println!(">> amount: {:?}", output.amount);
                    let balance = utxo_balances.entry(output_hex).or_insert(0);
                    *balance += output.amount;
                });

                // need to add balances?

                // tx.from.iter().for_each(|input| {
                //     input.on_chain_reorganization(&utxoset, longest_chain, input_slip_spendable)
                // });
                // tx.to.iter().for_each(|output| {
                //     output.on_chain_reorganization(&utxoset, longest_chain, output_slip_spendable)
                // });
            }
        }

        for (key, value) in utxo_balances {
            if (value > threshold) {
                println!("{}\t{}", key, value);
                writeln!(file, "{} {:?}", key, value);
            }
        }
    }

    pub async fn check_token_supply(&self) {
        println!("check_token_supply");
        let mut token_supply: Currency = 0;
        let mut current_supply: Currency = 0;
        let mut block_inputs_amount: Currency;
        let mut block_outputs_amount: Currency;
        let mut previous_block_treasury: Currency;
        let mut current_block_treasury: Currency = 0;
        let mut unpaid_but_uncollected: Currency = 0;
        let mut block_contains_fee_tx: bool;
        let mut block_fee_tx_index: usize = 0;

        let (blockchain, _blockchain_) =
            lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

        let latest_block_id = blockchain.get_latest_block_id();

        for i in 1..=latest_block_id {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            let block = blockchain.get_block(&block_hash).unwrap();

            block_inputs_amount = 0;
            block_outputs_amount = 0;
            block_contains_fee_tx = false;

            previous_block_treasury = current_block_treasury;
            current_block_treasury = block.treasury;

            for t in 0..block.transactions.len() {
                //
                // we ignore the inputs in staking / fee transactions as they have
                // been pulled from the staking treasury and are already technically
                // counted in the money supply as an output from a previous slip.
                // we only care about the difference in token supply represented by
                // the difference in the staking_treasury.
                //
                if block.transactions[t].transaction_type == TransactionType::Fee {
                    block_contains_fee_tx = true;
                    block_fee_tx_index = t as usize;
                } else {
                    for z in 0..block.transactions[t].from.len() {
                        block_inputs_amount += block.transactions[t].from[z].amount;
                    }
                    for z in 0..block.transactions[t].to.len() {
                        block_outputs_amount += block.transactions[t].to[z].amount;
                    }
                    println!("block_inputs_amount {}", block_inputs_amount);
                    println!("block_outputs_amount {}", block_outputs_amount);
                }

                //
                // block one sets circulation
                //
                if i == 1 {
                    token_supply = block_outputs_amount + block.treasury + block.staking_treasury;
                    current_supply = token_supply;
                    println!("token_supply {}", token_supply);
                    println!("current_supply {}", current_supply);
                } else {
                    //
                    // figure out how much is in circulation
                    //
                    if block_contains_fee_tx == false {
                        current_supply -= block_inputs_amount;
                        current_supply += block_outputs_amount;

                        unpaid_but_uncollected += block_inputs_amount;
                        unpaid_but_uncollected -= block_outputs_amount;

                        //
                        // treasury increases must come here uncollected
                        //
                        if current_block_treasury > previous_block_treasury {
                            unpaid_but_uncollected -=
                                current_block_treasury - previous_block_treasury;
                        }
                    } else {
                        //
                        // calculate total amount paid
                        //
                        let mut total_fees_paid: Currency = 0;
                        let fee_transaction = &block.transactions[block_fee_tx_index];
                        for output in fee_transaction.to.iter() {
                            total_fees_paid += output.amount;
                        }

                        current_supply -= block_inputs_amount;
                        current_supply += block_outputs_amount;
                        current_supply += total_fees_paid;

                        unpaid_but_uncollected += block_inputs_amount;
                        unpaid_but_uncollected -= block_outputs_amount;
                        unpaid_but_uncollected -= total_fees_paid;

                        //
                        // treasury increases must come here uncollected
                        //
                        if current_block_treasury > previous_block_treasury {
                            unpaid_but_uncollected -=
                                current_block_treasury - previous_block_treasury;
                        }
                    }

                    //
                    // token supply should be constant
                    //
                    let total_in_circulation = current_supply
                        + unpaid_but_uncollected
                        + block.treasury
                        + block.staking_treasury;

                    //
                    // we check that overall token supply has not changed
                    //
                    assert_eq!(total_in_circulation, token_supply);
                }
            }
        }
    }

    pub async fn create_tx(
        &mut self,
        public_key: &SaitoPublicKey,
        private_key: &SaitoPrivateKey,
        txs_number: usize,
        txs_amount: Currency,
        txs_fee: Currency,
    ) -> AHashMap<SaitoSignature, Transaction> {
        let mut transactions: AHashMap<SaitoSignature, Transaction> = Default::default();
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;

        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        for _i in 0..txs_number {
            let mut transaction;
            {
                let (mut wallet, _wallet_) = lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);

                transaction =
                    Transaction::create(&mut wallet, public_key, txs_amount, txs_fee, false)
                        .unwrap();
            }

            transaction.sign(&private_key);
            transaction.generate(&public_key, 0, 0);
            transactions.insert(transaction.signature, transaction);
        }

        return transactions;
    }

    //
    // create block
    //
    pub async fn create_block(
        &mut self,
        parent_hash: SaitoHash,
        timestamp: Timestamp,
        txs_number: usize,
        txs_amount: Currency,
        txs_fee: Currency,
        include_valid_golden_ticket: bool,
    ) -> Block {
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;
        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        let mut transactions = self
            .create_tx(&public_key, &private_key, txs_number, txs_amount, txs_fee)
            .await;

        if include_valid_golden_ticket {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let block = blockchain.get_block(&parent_hash).unwrap();
            let golden_ticket: GoldenTicket =
                Self::create_golden_ticket(self.wallet_lock.clone(), parent_hash, block.difficulty)
                    .await;
            let mut gttx: Transaction;
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

                gttx = Wallet::create_golden_ticket_transaction(
                    golden_ticket,
                    &wallet.public_key,
                    &wallet.private_key,
                )
                .await;
            }
            gttx.generate(&public_key, 0, 0);
            transactions.insert(gttx.signature, gttx);
        }

        let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
        let (mut blockchain, _blockchain_) =
            lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
        //
        // create block
        //
        let mut block = Block::create(
            &mut transactions,
            parent_hash,
            blockchain.borrow_mut(),
            timestamp,
            &public_key,
            &private_key,
            None,
            configs.deref(),
        )
        .await;
        block.generate();
        block.sign(&private_key);

        block
    }

    pub async fn create_golden_ticket(
        wallet: Arc<RwLock<Wallet>>,
        block_hash: SaitoHash,
        block_difficulty: u64,
    ) -> GoldenTicket {
        let public_key;
        {
            let (wallet, _wallet_) = lock_for_read!(wallet, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
        }
        let mut random_bytes = hash(&generate_random_bytes(32));

        let mut gt = GoldenTicket::create(block_hash, random_bytes, public_key);

        while !gt.validate(block_difficulty) {
            random_bytes = hash(&generate_random_bytes(32));
            gt = GoldenTicket::create(block_hash, random_bytes, public_key);
        }

        GoldenTicket::new(block_hash, random_bytes, public_key)
    }

    pub async fn initialize(&mut self, vip_transactions: u64, vip_amount: Currency) {
        let timestamp = create_timestamp();
        self.initialize_with_timestamp(vip_transactions, vip_amount, timestamp)
            .await;
    }

    //
    // initialize chain
    //
    // creates and adds the first block to the blockchain, with however many VIP
    // transactions are necessary
    pub async fn initialize_with_timestamp(
        &mut self,
        vip_transactions: u64,
        vip_amount: Currency,
        timestamp: Timestamp,
    ) {
        //
        // initialize timestamp
        //

        //
        // reset data dirs
        //
        tokio::fs::remove_dir_all("data/blocks").await;
        tokio::fs::create_dir_all("data/blocks").await.unwrap();
        tokio::fs::remove_dir_all("data/wallets").await;
        tokio::fs::create_dir_all("data/wallets").await.unwrap();

        //
        // create initial transactions
        //
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;
        {
            let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        //
        // create first block
        //
        let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

        //
        // generate UTXO-carrying VIP transactions
        //
        for _i in 0..vip_transactions {
            let mut tx = Transaction::create_vip_transaction(public_key, vip_amount);
            tx.generate(&public_key, 0, 0);
            tx.sign(&private_key);
            block.add_transaction(tx);
        }

        {
            let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
            // we have added VIP, so need to regenerate the merkle-root
            block.merkle_root =
                block.generate_merkle_root(configs.is_browser(), configs.is_spv_mode());
        }
        block.generate();
        block.sign(&private_key);

        assert!(verify_signature(
            &block.pre_hash,
            &block.signature,
            &block.creator,
        ));

        // and add first block to blockchain
        self.add_block(block).await;
    }
}

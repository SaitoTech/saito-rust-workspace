#[cfg(test)]
pub mod test {
    //
    // TestManager provides a set of functions to simplify the testing of dynamic
    // interactions, such as chain-reorganizations and/or other tests that require
    // the state of the chain itself to vary in complicated ways.
    //
    // Our goal with this file is to make it faster and easier to make tests more
    // succinct, by providing helper functions that can create chains of blocks
    // with valid transactions and add them to the blockchain in a systematic way
    // that also permits manual intercession.
    //
    //  - create_block
    //  - create_transaction
    //  - create_transactions
    //  - on_chain_reorganization
    //
    // generate_block 		<-- create a block
    // generate_block_and_metadata 	<--- create block with metadata (difficulty, has_golden ticket, etc.)
    // generate_transaction 	<-- create a transaction
    // add_block 			<-- create and add block to longest_chain
    // add_block_on_hash		<-- create and add block elsewhere on chain
    // on_chain_reorganization 	<-- test monetary policy
    //
    //
    use std::borrow::BorrowMut;
    use std::error::Error;
    use std::fmt::{Debug, Formatter};
    use std::fs;
    use std::io::{BufReader, Read, Write};
    use std::ops::Deref;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ahash::AHashMap;
    use log::{debug, info};
    use rand::rngs::OsRng;
    use secp256k1::Secp256k1;
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio::sync::RwLock;

    use crate::core::consensus::block::{Block, BlockType};
    use crate::core::consensus::blockchain::Blockchain;
    use crate::core::consensus::golden_ticket::GoldenTicket;
    use crate::core::consensus::mempool::Mempool;
    use crate::core::consensus::peer_collection::PeerCollection;
    use crate::core::consensus::slip::Slip;
    use crate::core::consensus::transaction::{Transaction, TransactionType};
    use crate::core::consensus::wallet::Wallet;
    use crate::core::defs::{
        push_lock, Currency, PrintForLog, SaitoHash, SaitoPrivateKey, SaitoPublicKey,
        SaitoSignature, Timestamp, UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
        LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET, PROJECT_PUBLIC_KEY,
    };
    use crate::core::io::network::Network;
    use crate::core::io::storage::Storage;
    use crate::core::mining_thread::MiningEvent;
    use crate::core::process::keep_time::KeepTime;
    use crate::core::util::configuration::{BlockchainConfig, Configuration, PeerConfig, Server};
    use crate::core::util::crypto::{generate_keys, generate_random_bytes, hash, verify_signature};
    use crate::core::util::test::test_io_handler::test::TestIOHandler;
    use crate::{lock_for_read, lock_for_write};

    pub fn create_timestamp() -> Timestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as Timestamp
    }

    pub const TEST_ISSUANCE_FILEPATH: &'static str = "../saito-rust/data/issuance/test/issuance";

    struct TestTimeKeeper {}

    impl KeepTime for TestTimeKeeper {
        fn get_timestamp_in_ms(&self) -> Timestamp {
            create_timestamp()
        }
    }

    pub struct TestManager {
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
        pub issuance_path: &'static str,
    }
    impl Default for TestManager {
        fn default() -> Self {
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

            let issuance_path = TestManager::get_test_issuance_file().unwrap();

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
                    Box::new(TestTimeKeeper {}),
                ),
                peers: peers.clone(),
                storage: Storage::new(Box::new(TestIOHandler::new())),
                sender_to_miner: sender_to_miner.clone(),
                receiver_in_miner,
                configs,
                issuance_path,
            }
        }
    }

    impl TestManager {
        pub fn get_test_issuance_file() -> Result<&'static str, std::io::Error> {
            let temp_dir = Path::new("./temp_test_directory").to_path_buf();
            fs::create_dir_all(&temp_dir)?;
            let source_path = Path::new(TEST_ISSUANCE_FILEPATH);
            // Read the existing counter from the file or initialize it to 1 if the file doesn't exist
            let issuance_counter_path = temp_dir.join("issuance_counter.txt");
            let counter = if issuance_counter_path.exists() {
                let mut file = BufReader::new(fs::File::open(&issuance_counter_path)?);
                let mut buffer = String::new();
                file.read_to_string(&mut buffer)?;
                buffer.trim().parse::<usize>().unwrap_or(1)
            } else {
                1
            };
            let target_filename = format!("issuance-{}.txt", counter);
            let target_path = temp_dir.join(target_filename);
            fs::copy(&source_path, &target_path)?;
            // Update the counter in the file for the next instance
            let mut file = fs::File::create(&issuance_counter_path)?;
            writeln!(file, "{}", counter + 1)?;

            let target_path_str = target_path
                .to_str()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Invalid path"))?;

            let static_str: &'static str = Box::leak(target_path_str.to_string().into_boxed_str());

            Ok(static_str)
        }

        pub async fn convert_issuance_to_hashmap(
            &self,
            filepath: &'static str,
        ) -> AHashMap<SaitoPublicKey, u64> {
            let buffer = self.storage.read(filepath).await.unwrap();
            let content = String::from_utf8(buffer).expect("Failed to convert to String");

            let mut issuance_map = AHashMap::new();
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let amount: u64 = match parts[0].parse() {
                        Ok(num) => num,
                        Err(_) => continue, // skip lines with invalid numbers
                    };

                    let address_key = if amount < 25000 {
                        // Default public key when amount is less than 25000

                        SaitoPublicKey::from_base58(PROJECT_PUBLIC_KEY)
                            .expect("Failed to decode Base58")
                    } else {
                        SaitoPublicKey::from_base58(parts[1]).expect("Failed to decode Base58")
                    };

                    if address_key.len() == 33 {
                        let mut address: [u8; 33] = [0; 33];
                        address.copy_from_slice(&address_key);
                        *issuance_map.entry(address).or_insert(0) += amount; // add the amount to the existing value or set it if not present
                    }
                }
            }
            issuance_map
        }
        pub async fn wait_for_mining_event(&mut self) {
            self.receiver_in_miner
                .recv()
                .await
                .expect("mining event receive failed");
        }

        /// add block to blockchain
        pub async fn add_block(&mut self, block: Block) {
            debug!("adding block to test manager blockchain");

            {
                let (configs, _configs_) = lock_for_write!(self.configs, LOCK_ORDER_CONFIGS);
                let (mut blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
                let (mut mempool, _mempool_) =
                    lock_for_write!(self.mempool_lock, LOCK_ORDER_MEMPOOL);

                let _ = blockchain
                    .add_block(
                        block,
                        Some(&self.network),
                        &mut self.storage,
                        None,
                        &mut mempool,
                        configs.deref(),
                    )
                    .await;

                self.latest_block_hash = blockchain.last_block_hash;
            }
        }

        // check that the blockchain connects properly
        pub async fn check_blockchain(&self) {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            for i in 1..blockchain.blocks.len() {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(i as u64);

                let previous_block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_at_block_id((i as u64) - 1);

                let block = blockchain.get_block_sync(&block_hash);
                let previous_block = blockchain.get_block_sync(&previous_block_hash);

                if block_hash == [0; 32] {
                    assert!(block.is_none());
                } else {
                    assert!(block.is_some());
                    if i != 1 && previous_block_hash != [0; 32] {
                        assert!(previous_block.is_some());
                        assert_eq!(
                            block.unwrap().previous_block_hash,
                            previous_block.unwrap().hash
                        );
                    }
                }
            }
        }

        // check that everything spendable in the main UTXOSET is spendable on the longest
        // chain and vice-versa.
        pub async fn check_utxoset(&self) {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let mut utxoset: UtxoSet = AHashMap::new();
            let latest_block_id = blockchain.get_latest_block_id();
            // let first_block_id = latest_block_id - GENESIS_PERIOD + 1;

            info!("---- check utxoset ");
            for i in 1..=latest_block_id {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(i);
                let mut block = blockchain.get_block(&block_hash).unwrap().clone();
                block
                    .upgrade_block_to_block_type(BlockType::Full, &self.storage, false)
                    .await;
                info!(
                    "WINDING ID HASH - {:?} {:?} with txs : {:?} block_type : {:?}",
                    block.id,
                    block_hash.to_hex(),
                    block.transactions.len(),
                    block.block_type
                );

                for j in 0..block.transactions.len() {
                    block.transactions[j].on_chain_reorganization(&mut utxoset, true);
                    debug!(
                        "from : {:?} to : {:?} utxo len : {:?}",
                        block.transactions[j].from.len(),
                        block.transactions[j].to.len(),
                        utxoset.len()
                    );
                }
            }

            // check main utxoset matches longest-chain
            for (key, value) in &blockchain.utxoset {
                assert!(*value);
                match utxoset.get(key) {
                    Some(value2) => {
                        // everything spendable in blockchain.utxoset should be spendable on longest-chain
                        assert!(*value);
                        assert_eq!(*value, *value2);
                    }
                    None => {
                        let slip = Slip::parse_slip_from_utxokey(key);
                        info!(
                            "block : {:?} tx : {:?} amount : {:?} type : {:?}",
                            slip.block_id, slip.tx_ordinal, slip.amount, slip.slip_type
                        );
                        panic!("utxoset value should be false for key : {:?}. generated utxo size : {:?}. current utxo size : {:?}",
                               key.to_hex(), utxoset.len(), blockchain.utxoset.len());
                    }
                }
            }

            // check longest-chain matches utxoset
            for (key, value) in &utxoset {
                //info!("{:?} / {}", key, value);
                match blockchain.utxoset.get(key) {
                    Some(value2) => {
                        // everything spendable in longest-chain should be spendable on blockchain.utxoset
                        if *value == true {
                            //                        info!("comparing {} and {}", value, value2);
                            assert_eq!(value, value2);
                        } else {
                            //
                            // everything spent in longest-chain should be spendable on blockchain.utxoset
                            //
                            // if *value > 1 {
                            //     //                            info!("comparing {} and {}", value, value2);
                            //     assert_eq!(value, value2);
                            // } else {
                            //     //
                            //     // unspendable (0) does not need to exist
                            //     //
                            // }
                        }
                    }
                    None => {
                        info!("comparing {:?} with expected value {}", key, value);
                        info!("Value does not exist in actual blockchain!");
                        assert_eq!(1, 2);
                    }
                }
            }
        }

        pub async fn check_token_supply(&self) {
            let mut token_supply: Currency = 0;
            let mut current_supply: Currency = 0;
            let mut block_inputs: Currency;
            let mut block_outputs: Currency;
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
                    .get_longest_chain_block_hash_at_block_id(i);
                let block = blockchain.get_block(&block_hash).unwrap();

                block_inputs = 0;
                block_outputs = 0;
                block_contains_fee_tx = false;

                previous_block_treasury = current_block_treasury;
                current_block_treasury = block.limbo;

                for t in 0..block.transactions.len() {
                    // we ignore the inputs in staking / fee transactions as they have
                    // been pulled from the staking treasury and are already technically
                    // counted in the money supply as an output from a previous slip.
                    // we only care about the difference in token supply represented by
                    // the difference in the staking_treasury.
                    if block.transactions[t].transaction_type == TransactionType::Fee {
                        block_contains_fee_tx = true;
                        block_fee_tx_index = t;
                    } else {
                        for z in 0..block.transactions[t].from.len() {
                            block_inputs += block.transactions[t].from[z].amount;
                        }
                        for z in 0..block.transactions[t].to.len() {
                            block_outputs += block.transactions[t].to[z].amount;
                        }
                    }

                    // block one sets circulation
                    if i == 1 {
                        token_supply = block_outputs + block.limbo + block.treasury;
                        current_supply = token_supply;
                    } else {
                        current_supply = (current_supply as i128 + block_outputs as i128
                            - block_inputs as i128)
                            as Currency;
                        unpaid_but_uncollected =
                            (unpaid_but_uncollected as i128 + block_inputs as i128
                                - block_outputs as i128) as Currency;

                        // figure out how much is in circulation
                        if block_contains_fee_tx {
                            // calculate total amount paid
                            let mut total_fees_paid: Currency = 0;
                            let fee_transaction = &block.transactions[block_fee_tx_index];
                            for output in fee_transaction.to.iter() {
                                total_fees_paid += output.amount;
                            }

                            current_supply += total_fees_paid;
                            unpaid_but_uncollected -= total_fees_paid;
                        }
                        // treasury increases must come here uncollected
                        if current_block_treasury > previous_block_treasury {
                            unpaid_but_uncollected = (unpaid_but_uncollected as i128
                                - (current_block_treasury as i128
                                    - previous_block_treasury as i128))
                                as Currency;
                        }

                        assert_eq!(
                            block.limbo, 0,
                            "block.limbo should always be 0 until it's implemented"
                        );

                        println!("i : {:?} current_supply : {:?} unpaid_but_uncollected : {:?} block.limbo : {:?} block.treasury : {:?}",
                                 i,current_supply,unpaid_but_uncollected,block.limbo,block.treasury);

                        // token supply should be constant
                        let total_in_circulation =
                            current_supply + unpaid_but_uncollected + block.limbo + block.treasury;

                        // we check that overall token supply has not changed
                        assert_eq!(total_in_circulation, token_supply,
                                   "total_in_circulation : {:?} token_supply : {:?} unpaid_but_uncollected : {:?} block.treasury : {:?} block.staking_treasury : {:?} current_supply : {:?}",
                                   total_in_circulation,
                                   token_supply,
                                   unpaid_but_uncollected,
                                   block.limbo,
                                   block.treasury,
                                   current_supply);
                    }
                }
            }
        }

        // create block
        pub async fn create_block(
            &mut self,
            parent_hash: SaitoHash,
            timestamp: Timestamp,
            txs_count: usize,
            txs_amount: Currency,
            txs_fee: Currency,
            include_valid_golden_ticket: bool,
        ) -> Block {
            let mut transactions: AHashMap<SaitoSignature, Transaction> = Default::default();
            let private_key: SaitoPrivateKey;
            let public_key: SaitoPublicKey;

            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

                public_key = wallet.public_key;
                private_key = wallet.private_key;
            }

            for _i in 0..txs_count {
                let mut transaction;
                {
                    let (mut wallet, _wallet_) =
                        lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);

                    transaction = Transaction::create(
                        &mut wallet,
                        public_key,
                        txs_amount,
                        txs_fee,
                        false,
                        None,
                    )
                    .unwrap();
                }

                transaction.sign(&private_key);
                transaction.generate(&public_key, 0, 0);
                transactions.insert(transaction.signature, transaction);
            }

            if include_valid_golden_ticket {
                let (blockchain, _blockchain_) =
                    lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

                let block = blockchain.get_block(&parent_hash).unwrap_or_else(|| {
                    panic!("couldn't find block for hash : {:?}", parent_hash.to_hex())
                });
                let golden_ticket: GoldenTicket = Self::create_golden_ticket(
                    self.wallet_lock.clone(),
                    parent_hash,
                    block.difficulty,
                )
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

            // create block
            let mut block = Block::create(
                &mut transactions,
                parent_hash,
                blockchain.borrow_mut(),
                timestamp,
                &public_key,
                &private_key,
                None,
                configs.deref(),
                &self.storage,
            )
            .await;
            block.generate();
            block.sign(&private_key);

            block
        }

        pub async fn create_golden_ticket(
            wallet_lock: Arc<RwLock<Wallet>>,
            block_hash: SaitoHash,
            block_difficulty: u64,
        ) -> GoldenTicket {
            let public_key;
            {
                let (wallet, _wallet_) = lock_for_read!(wallet_lock, LOCK_ORDER_WALLET);

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

        pub async fn get_balance(&self) -> u64 {
            let wallet_lock = self.get_wallet_lock();
            let (wallet, _wallet_) = lock_for_read!(wallet_lock, LOCK_ORDER_WALLET);
            let my_balance = wallet.get_available_balance();

            my_balance
        }

        pub fn get_mempool_lock(&self) -> Arc<RwLock<Mempool>> {
            return self.mempool_lock.clone();
        }

        pub fn get_wallet_lock(&self) -> Arc<RwLock<Wallet>> {
            return self.wallet_lock.clone();
        }

        // pub async fn get_wallet(&self) -> tokio::sync::RwLockReadGuard<'_, Wallet> {
        //     let wallet;
        //     let _wallet_;
        //     (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);
        //     // return wallet;
        // }

        pub fn get_blockchain_lock(&self) -> Arc<RwLock<Blockchain>> {
            return self.blockchain_lock.clone();
        }

        pub async fn get_latest_block_hash(&self) -> SaitoHash {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            blockchain.blockring.get_latest_block_hash()
        }
        pub async fn get_latest_block(&self) -> Block {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            let block = blockchain
                .get_latest_block()
                .expect("latest block should exist")
                .clone();
            block
        }
        pub async fn get_latest_block_id(&self) -> u64 {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            blockchain.blockring.get_latest_block_id()
        }

        pub async fn initialize(&mut self, issuance_transactions: u64, issuance_amount: Currency) {
            let timestamp = create_timestamp();
            self.initialize_with_timestamp(issuance_transactions, issuance_amount, timestamp)
                .await;
        }

        // initialize chain from slips and add some amount my public key
        //
        pub async fn initialize_from_slips_and_value(&mut self, slips: Vec<Slip>, amount: u64) {
            //
            // reset data dirs
            //
            let _ = tokio::fs::remove_dir_all("data/blocks").await;
            tokio::fs::create_dir_all("data/blocks").await.unwrap();
            let _ = tokio::fs::remove_dir_all("data/wallets").await;
            tokio::fs::create_dir_all("data/wallets").await.unwrap();

            let private_key: SaitoPrivateKey;
            let my_public_key: SaitoPublicKey;
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);
                private_key = wallet.private_key;
                my_public_key = wallet.public_key;
            }

            // create first block
            let timestamp = create_timestamp();
            let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

            for slip in slips {
                let mut tx: Transaction =
                    Transaction::create_issuance_transaction(slip.public_key, slip.amount);
                tx.generate(&slip.public_key, 0, 0);
                tx.sign(&private_key);
                block.add_transaction(tx);
            }

            // add some value to my own public key
            let mut tx = Transaction::create_issuance_transaction(my_public_key, amount);

            tx.generate(&my_public_key, 0, 0);
            tx.sign(&private_key);
            block.add_transaction(tx);

            {
                let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
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

        // initialize chain from just slips properties
        pub async fn initialize_from_slips(&mut self, slips: Vec<Slip>) {
            //
            // initialize timestamp
            //
            let timestamp = create_timestamp();
            //
            // reset data dirs
            //
            let _ = tokio::fs::remove_dir_all("data/blocks").await;
            tokio::fs::create_dir_all("data/blocks").await.unwrap();
            let _ = tokio::fs::remove_dir_all("data/wallets").await;
            tokio::fs::create_dir_all("data/wallets").await.unwrap();

            //
            // create initial transactions
            //
            let private_key: SaitoPrivateKey;
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);
                private_key = wallet.private_key;
            }

            //
            // create first block
            //

            let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

            //
            // generate UTXO-carrying VIP transactions
            //
            for slip in slips {
                let mut tx = Transaction::create_issuance_transaction(slip.public_key, slip.amount);
                tx.generate(&slip.public_key, 0, 0);
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
        pub async fn initialize_with_timestamp(
            &mut self,
            issuance_transactions: u64,
            issuance_amount: Currency,
            timestamp: Timestamp,
        ) {
            // reset data dirs
            let _ = tokio::fs::remove_dir_all("data/blocks").await;
            let _ = tokio::fs::create_dir_all("data/blocks").await;
            let _ = tokio::fs::remove_dir_all("data/wallets").await;
            let _ = tokio::fs::create_dir_all("data/wallets").await;

            // create initial transactions
            let private_key: SaitoPrivateKey;
            let public_key: SaitoPublicKey;
            {
                let (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);

                public_key = wallet.public_key;
                private_key = wallet.private_key;
            }

            // create first block
            let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

            // generate UTXO-carrying transactions
            for _i in 0..issuance_transactions {
                let mut tx = Transaction::create_issuance_transaction(public_key, issuance_amount);
                tx.generate(&public_key, 0, 0);
                tx.sign(&private_key);
                block.add_transaction(tx);
            }

            {
                let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);
                // we have added txs, so need to regenerate the merkle-root
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

        //create a genesis block for testing
        pub async fn create_test_gen_block(&mut self, amount: u64) {
            debug!("create_test_gen_block");
            let wallet_read = self.wallet_lock.read().await;
            let mut tx = Transaction::create_issuance_transaction(wallet_read.public_key, amount);
            tx.sign(&wallet_read.private_key);
            drop(wallet_read);
            let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);

            let (mut blockchain, _blockchain_) =
                lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            let (mut mempool, _mempool_) = lock_for_write!(self.mempool_lock, LOCK_ORDER_MEMPOOL);

            mempool
                .add_transaction_if_validates(tx.clone(), &blockchain)
                .await;

            let timestamp = create_timestamp();

            let genblock: Block = mempool
                .bundle_genesis_block(&mut blockchain, timestamp, configs.deref(), &self.storage)
                .await;
            let _res = blockchain
                .add_block(
                    genblock,
                    Some(&self.network),
                    &mut self.storage,
                    Some(self.sender_to_miner.clone()),
                    &mut mempool,
                    configs.deref(),
                )
                .await;
        }

        //convenience function assuming longest chain
        pub async fn balance_map(&mut self) -> AHashMap<SaitoPublicKey, u64> {
            let (blockchain, _blockchain_) =
                lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let mut utxo_balances: AHashMap<SaitoPublicKey, u64> = AHashMap::new();

            let latest_id = blockchain.get_latest_block_id();
            for i in 1..=latest_id {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(i as u64);
                let block = blockchain.get_block(&block_hash).unwrap().clone();
                for j in 0..block.transactions.len() {
                    let tx = &block.transactions[j];

                    tx.from.iter().for_each(|input| {
                        utxo_balances
                            .entry(input.public_key)
                            .and_modify(|e| *e -= input.amount)
                            .or_insert(0);
                    });

                    tx.to.iter().for_each(|output| {
                        utxo_balances
                            .entry(output.public_key)
                            .and_modify(|e| *e += output.amount)
                            .or_insert(output.amount);
                    });
                }
            }
            utxo_balances
        }

        pub async fn transfer_value_to_public_key(
            &mut self,
            to_public_key: SaitoPublicKey,
            amount: u64,
            timestamp_addition: u64,
        ) -> Result<(), Box<dyn Error>> {
            let latest_block_hash = self.get_latest_block_hash().await;
            // dbg!(latest_block_hash);
            {
                let (_blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

                // dbg!(blockchain.get_latest_block());
                // latest_block_hash = blockchain.blockring.get_latest_block_hash()
            }

            let timestamp = create_timestamp();

            let mut block = self
                .create_block(
                    latest_block_hash,
                    timestamp + timestamp_addition,
                    0,
                    0,
                    0,
                    false,
                )
                .await;

            let private_key;
            let from_public_key;

            {
                let wallet;
                let _wallet_;
                (wallet, _wallet_) = lock_for_read!(self.wallet_lock, LOCK_ORDER_WALLET);
                from_public_key = wallet.public_key;
                private_key = wallet.private_key;
                let mut tx = Transaction::create(
                    &mut wallet.clone(),
                    to_public_key,
                    amount,
                    0,
                    false,
                    None,
                )?;
                tx.sign(&private_key);
                block.add_transaction(tx);
            }

            {
                let (configs, _configs_) = lock_for_read!(self.configs, LOCK_ORDER_CONFIGS);

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
            self.add_block(block).await;

            Ok(())
        }

        pub fn generate_random_public_key() -> SaitoPublicKey {
            let secp = Secp256k1::new();
            let (_secret_key, public_key) = secp.generate_keypair(&mut OsRng);
            let serialized_key: SaitoPublicKey = public_key.serialize();
            serialized_key
        }
    }

    impl Drop for TestManager {
        fn drop(&mut self) {
            // Cleanup: Remove the temporary directory and its contents
            if let Err(err) = fs::remove_dir_all("./temp_test_directory") {
                eprintln!("Error cleaning up: {}", err);
            }
        }
    }

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

        fn get_blockchain_configs(&self) -> Option<BlockchainConfig> {
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
}

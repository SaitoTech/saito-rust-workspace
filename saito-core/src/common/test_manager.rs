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
    use std::ops::Deref;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ahash::AHashMap;
    use log::{debug, info};
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio::sync::RwLock;

    use crate::common::defs::{
        push_lock, Currency, PrintForLog, SaitoHash, SaitoPrivateKey, SaitoPublicKey,
        SaitoSignature, Timestamp, UtxoSet, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
        LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET, PROJECT_PUBLIC_KEY,
    };
    use crate::common::keep_time::KeepTime;
    use crate::common::test_io_handler::test::TestIOHandler;
    use crate::core::data::block::Block;
    use crate::core::data::blockchain::Blockchain;
    use crate::core::data::configuration::{BlockchainConfig, Configuration, PeerConfig, Server};
    use crate::core::data::crypto::{generate_keys, generate_random_bytes, hash, verify_signature};
    use crate::core::data::golden_ticket::GoldenTicket;
    use crate::core::data::mempool::Mempool;
    use crate::core::data::network::Network;
    use crate::core::data::peer_collection::PeerCollection;
    use crate::core::data::slip::Slip;
    use crate::core::data::storage::Storage;
    use crate::core::data::transaction::{Transaction, TransactionType};
    use crate::core::data::wallet::Wallet;
    use crate::core::mining_thread::MiningEvent;
    use crate::{lock_for_read, lock_for_write};
    use rand::rngs::OsRng;
    use secp256k1::{Secp256k1, SecretKey};

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
    }

    impl TestManager {
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
                    Box::new(TestTimeKeeper {}),
                ),
                peers: peers.clone(),
                storage: Storage::new(Box::new(TestIOHandler::new())),
                sender_to_miner: sender_to_miner.clone(),
                receiver_in_miner,
                configs,
            }
        }

        // generates utxo hashmap from an issuance file
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

        //
        // add block to blockchain
        //
        pub async fn add_block(&mut self, block: Block) {
            debug!("adding block to test manager blockchain");

            {
                let (configs, _configs_) = lock_for_write!(self.configs, LOCK_ORDER_CONFIGS);
                let (mut blockchain, _blockchain_) =
                    lock_for_write!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
                let (mut mempool, _mempool_) =
                    lock_for_write!(self.mempool_lock, LOCK_ORDER_MEMPOOL);

                blockchain
                    .add_block(
                        block,
                        Some(&mut self.network),
                        &mut self.storage,
                        Some(self.sender_to_miner.clone()),
                        &mut mempool,
                        configs.deref(),
                    )
                    .await;

                dbg!("block added to test manager blockchain");
            }
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

        //
        // check that everything spendable in the main UTXOSET is spendable on the longest
        // chain and vice-versa.
        //
        pub async fn check_utxoset(&self) {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let mut utxoset: UtxoSet = AHashMap::new();
            let latest_block_id = blockchain.get_latest_block_id();

            info!("---- check utxoset ");
            for i in 1..=latest_block_id {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(i);
                let block = blockchain.get_block(&block_hash).unwrap();
                info!(
                    "WINDING ID HASH - {} {:?} with txs : {:?} block_type : {:?}",
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

            //
            // check main utxoset matches longest-chain
            //
            for (key, value) in &blockchain.utxoset {
                match utxoset.get(key) {
                    Some(value2) => {
                        //
                        // everything spendable in blockchain.utxoset should be spendable on longest-chain
                        //
                        if *value == true {
                            //info!("for key: {:?}", key);
                            //info!("comparing {} and {}", value, value2);
                            assert_eq!(value, value2);
                        } else {
                            //
                            // everything spent in blockchain.utxoset should be spent on longest-chain
                            //
                            // if *value > 1 {
                            //info!("comparing key: {:?}", key);
                            //info!("comparing blkchn {} and sanitycheck {}", value, value2);
                            // assert_eq!(value, value2);
                            // } else {
                            //
                            // unspendable (0) does not need to exist
                            //
                            // }
                        }
                    }
                    None => {
                        //
                        // if the value is 0, the token is unspendable on the main chain and
                        // it may still be in the UTXOSET simply because it was not removed
                        // but rather set to an unspendable value. These entries will be
                        // removed on purge, although we can look at deleting them on unwind
                        // as well if that is reasonably efficient.
                        assert!(!*value, "utxoset value should be false for key : {:?}. generated utxo size : {:?}. current utxo size : {:?}", key.to_hex(), utxoset.len(), blockchain.utxoset.len());
                        // if *value == true {
                        //     //info!("Value does not exist in actual blockchain!");
                        //     //info!("comparing {:?} with on-chain value {}", key, value);
                        //     assert_eq!(1, 2);
                        // }
                    }
                }
            }

            //
            // check longest-chain matches utxoset
            //
            for (key, value) in &utxoset {
                //info!("{:?} / {}", key, value);
                match blockchain.utxoset.get(key) {
                    Some(value2) => {
                        //
                        // everything spendable in longest-chain should be spendable on blockchain.utxoset
                        //
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
                    .get_longest_chain_block_hash_at_block_id(i as u64);
                let block = blockchain.get_block(&block_hash).unwrap();

                block_inputs = 0;
                block_outputs = 0;
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
                            block_inputs += block.transactions[t].from[z].amount;
                        }
                        for z in 0..block.transactions[t].to.len() {
                            block_outputs += block.transactions[t].to[z].amount;
                        }
                    }

                    //
                    // block one sets circulation
                    //
                    if i == 1 {
                        token_supply = block_outputs + block.treasury + block.staking_treasury;
                        current_supply = token_supply;
                    } else {
                        //
                        // figure out how much is in circulation
                        //
                        if block_contains_fee_tx == false {
                            current_supply -= block_inputs;
                            current_supply += block_outputs;

                            unpaid_but_uncollected += block_inputs;
                            unpaid_but_uncollected -= block_outputs;

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

                            current_supply -= block_inputs;
                            current_supply += block_outputs;
                            current_supply += total_fees_paid;

                            unpaid_but_uncollected += block_inputs;
                            unpaid_but_uncollected -= block_outputs;
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

                let block = blockchain.get_block(&parent_hash).unwrap();
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
            let block = blockchain.get_latest_block().unwrap().clone();
            return block;
        }
        pub async fn get_latest_block_id(&self) -> u64 {
            let (blockchain, _blockchain_) =
                lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            blockchain.blockring.get_latest_block_id()
        }

        pub async fn initialize(&mut self, vip_transactions: u64, vip_amount: Currency) {
            let timestamp = create_timestamp();
            self.initialize_with_timestamp(vip_transactions, vip_amount, timestamp)
                .await;
        }

        //
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
                    Transaction::create_vip_transaction(slip.public_key, slip.amount);
                tx.generate(&slip.public_key, 0, 0);
                tx.sign(&private_key);
                block.add_transaction(tx);
            }

            // add some value to my own public key
            let mut tx = Transaction::create_vip_transaction(my_public_key, amount);

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

        //
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
                let mut tx = Transaction::create_vip_transaction(slip.public_key, slip.amount);
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
            let _ = tokio::fs::remove_dir_all("data/blocks").await;
            tokio::fs::create_dir_all("data/blocks").await.unwrap();
            let _ = tokio::fs::remove_dir_all("data/wallets").await;
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
                .bundle_genesis_block(&mut blockchain, timestamp, configs.deref())
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
            let (secret_key, public_key) = secp.generate_keypair(&mut OsRng);
            let serialized_key: SaitoPublicKey = public_key.serialize();
            serialized_key
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

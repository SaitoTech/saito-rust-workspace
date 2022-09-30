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
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ahash::AHashMap;
    use rayon::prelude::*;
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio::sync::RwLock;
    use tracing::{debug, info};

    use crate::common::defs::{SaitoHash, SaitoPrivateKey, SaitoPublicKey, UtxoSet};
    use crate::common::test_io_handler::test::TestIOHandler;
    use crate::core::data::block::Block;
    use crate::core::data::blockchain::Blockchain;

    use crate::core::data::crypto::{generate_random_bytes, hash, verify_hash};
    use crate::core::data::golden_ticket::GoldenTicket;
    use crate::core::data::mempool::Mempool;
    use crate::core::data::network::Network;
    use crate::core::data::peer_collection::PeerCollection;
    use crate::core::data::storage::Storage;
    use crate::core::data::transaction::{Transaction, TransactionType};
    use crate::core::data::wallet::Wallet;
    use crate::core::mining_event_processor::MiningEvent;
    use crate::{
        log_read_lock_receive, log_read_lock_request, log_write_lock_receive,
        log_write_lock_request,
    };

    pub fn create_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
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
    }

    impl TestManager {
        pub fn new() -> Self {
            let wallet = Wallet::new();
            let public_key = wallet.public_key.clone();
            let private_key = wallet.private_key.clone();
            let peers = Arc::new(RwLock::new(PeerCollection::new()));
            let wallet_lock = Arc::new(RwLock::new(wallet));
            let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
            let mempool_lock = Arc::new(RwLock::new(Mempool::new(public_key, private_key)));
            let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(1000);

            Self {
                wallet_lock: wallet_lock.clone(),
                blockchain_lock: blockchain_lock,
                mempool_lock: mempool_lock,
                latest_block_hash: [0; 32],
                network: Network::new(
                    Box::new(TestIOHandler::new()),
                    peers.clone(),
                    wallet_lock.clone(),
                ),
                peers: peers.clone(),
                storage: Storage::new(Box::new(TestIOHandler::new())),
                sender_to_miner: sender_to_miner.clone(),
                receiver_in_miner,
            }
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
            log_write_lock_request!("test_manager:add_block::blockchain");
            let mut blockchain = self.blockchain_lock.write().await;
            log_write_lock_receive!("test_manager:add_block::blockchain");
            log_write_lock_request!("test_manager:add_block::mempool");
            let mut mempool = self.mempool_lock.write().await;
            log_write_lock_receive!("test_manager:add_block::mempool");
            blockchain
                .add_block(
                    block,
                    &mut self.network,
                    &mut self.storage,
                    self.sender_to_miner.clone(),
                    &mut mempool,
                )
                .await;
            debug!("block added to test manager blockchain");
        }

        //
        // check that the blockchain connects properly
        //
        pub async fn check_blockchain(&self) {
            log_read_lock_request!("blockchain");
            let blockchain = self.blockchain_lock.read().await;
            log_read_lock_receive!("blockchain");

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

        //
        // check that everything spendable in the main UTXOSET is spendable on the longest
        // chain and vice-versa.
        //
        pub async fn check_utxoset(&self) {
            let blockchain = self.blockchain_lock.read().await;
            let mut utxoset: UtxoSet = AHashMap::new();
            let latest_block_id = blockchain.get_latest_block_id();

            info!("---- check utxoset ");
            for i in 1..=latest_block_id {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_by_block_id(i as u64);
                info!("WINDING ID HASH - {} {:?}", i, block_hash);
                let block = blockchain.get_block(&block_hash).unwrap();
                for j in 0..block.transactions.len() {
                    block.transactions[j].on_chain_reorganization(&mut utxoset, true, i as u64);
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
                        //
                        if *value == true {
                            //info!("Value does not exist in actual blockchain!");
                            //info!("comparing {:?} with on-chain value {}", key, value);
                            assert_eq!(1, 2);
                        }
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
            let mut token_supply: u64 = 0;
            let mut current_supply: u64 = 0;
            let mut block_inputs: u64;
            let mut block_outputs: u64;
            let mut previous_block_treasury: u64;
            let mut current_block_treasury: u64 = 0;
            let mut unpaid_but_uncollected: u64 = 0;
            let mut block_contains_fee_tx: u64;
            let mut block_fee_tx_index: usize = 0;

            let blockchain = self.blockchain_lock.read().await;
            let latest_block_id = blockchain.get_latest_block_id();

            for i in 1..=latest_block_id {
                let block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_by_block_id(i as u64);
                let block = blockchain.get_block(&block_hash).unwrap();

                block_inputs = 0;
                block_outputs = 0;
                block_contains_fee_tx = 0;

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
                        block_contains_fee_tx = 1;
                        block_fee_tx_index = t as usize;
                    } else {
                        for z in 0..block.transactions[t].inputs.len() {
                            block_inputs += block.transactions[t].inputs[z].amount;
                        }
                        for z in 0..block.transactions[t].outputs.len() {
                            block_outputs += block.transactions[t].outputs[z].amount;
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
                        if block_contains_fee_tx == 0 {
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
                            let mut total_fees_paid: u64 = 0;
                            let fee_transaction = &block.transactions[block_fee_tx_index];
                            for output in fee_transaction.outputs.iter() {
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
            timestamp: u64,
            txs_number: usize,
            txs_amount: u64,
            txs_fee: u64,
            include_valid_golden_ticket: bool,
        ) -> Block {
            let mut transactions: Vec<Transaction> = vec![];
            let private_key: SaitoPrivateKey;
            let public_key: SaitoPublicKey;

            {
                let wallet = self.wallet_lock.read().await;
                public_key = wallet.public_key;
                private_key = wallet.private_key;
            }

            for _i in 0..txs_number {
                let mut transaction;
                {
                    log_write_lock_request!("test_manager:create_block::wallet");
                    let mut wallet = self.wallet_lock.write().await;
                    log_write_lock_receive!("test_manager:create_block::wallet");
                    transaction = Transaction::create(&mut wallet, public_key, txs_amount, txs_fee);
                }

                transaction.sign(&private_key);
                transaction.generate(&public_key, 0, 0);
                transactions.push(transaction);
            }

            if include_valid_golden_ticket {
                let blockchain = self.blockchain_lock.read().await;
                let block = blockchain.get_block(&parent_hash).unwrap();
                let golden_ticket: GoldenTicket = Self::create_golden_ticket(
                    self.wallet_lock.clone(),
                    parent_hash,
                    block.difficulty,
                )
                .await;
                let mut gttx: Transaction;
                {
                    let mut wallet = self.wallet_lock.write().await;
                    gttx = Wallet::create_golden_ticket_transaction(
                        golden_ticket,
                        &wallet.public_key,
                        &wallet.private_key,
                    )
                    .await;
                }
                gttx.generate(&public_key, 0, 0);
                transactions.push(gttx);
            }

            //
            // create block
            //
            let mut block = Block::create(
                &mut transactions,
                parent_hash,
                self.blockchain_lock.clone().write().await.borrow_mut(),
                timestamp,
                &public_key,
                &private_key,
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
                log_read_lock_request!("wallet");
                let wallet = wallet.read().await;
                log_read_lock_receive!("wallet");
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

        pub async fn initialize(&mut self, vip_transactions: u64, vip_amount: u64) {
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
            vip_amount: u64,
            timestamp: u64,
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
                let wallet = self.wallet_lock.read().await;
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

            // we have added VIP, so need to regenerate the merkle-root
            block.merkle_root = block.generate_merkle_root();
            block.generate();
            block.sign(&private_key);

            assert!(verify_hash(
                &block.pre_hash,
                &block.signature,
                &block.creator
            ));

            // and add first block to blockchain
            self.add_block(block).await;
        }

        /***********************


            pub async fn generate_block_and_metadata(
                &mut self,
                parent_hash: SaitoHash,
                timestamp: u64,
                vip_transactions: usize,
                normal_transactions: usize,
                golden_ticket: bool,
                additional_transactions: Vec<Transaction>,
            ) -> Block {
                let mut block = self
                    .generate_block(
                        parent_hash,
                        timestamp,
                        vip_transactions,
                        normal_transactions,
                        golden_ticket,
                        additional_transactions,
                    )
                    .await;
                block.generate();
                block
            }


            //
            // add block on parent hash
            //
            pub async fn add_block_on_hash(
                &mut self,
                timestamp: u64,
                vip_txs: usize,
                normal_txs: usize,
                has_golden_ticket: bool,
                additional_txs: Vec<Transaction>,
                parent_hash: SaitoHash,
            ) -> SaitoHash {
                let mut block = self
                    .generate_block_and_metadata(
                        parent_hash,
                        timestamp,
                        vip_txs,
                        normal_txs,
                        has_golden_ticket,
                        additional_txs,
                    )
                    .await;

                let private_key: SaitoPrivateKey;
                let public_key: SaitoPublicKey;

                {
                    let wallet = self.wallet_lock.read().await;
                    public_key = wallet.get_public_key();
                    private_key = wallet.get_private_key();
                }
                block.sign(private_key);
            block.generate();

                self.latest_block_hash = block.get_hash();
                Self::add_block_to_blockchain(
                    self.blockchain_lock.clone(),
                    block,
                    &mut self.network,
                    &mut self.storage,
                )
                .await;
                self.latest_block_hash
            }
            pub async fn add_block_to_blockchain(
                blockchain_lock: Arc<RwLock<Blockchain>>,
                block: Block,
                network: &Network,
                storage: &mut Storage,
                sender: Sender<MiningEvent>,
            ) {
                let mut blockchain = blockchain_lock.write().await;
                blockchain
                    .add_block(block, network, storage, sender.clone())
                    .await;
            }
            //
            // generate_blockchain can be used to add multiple chains of blocks that are not
            // on the longest-chain, and thus will attempt to create transactions that reflect
            // the UTXOSET on the longest-chain at their time of creation.
            //
            // in order to prevent this from being a problem, this function is limited to
            // including a golden ticket every block so as to ensure there is no staking
            // payout in fork-block-5 that is created given expected state of staking table
            // on the other fork. There are no NORMAL transactions permitted and the golden
            // ticket is required every block.
            //
            pub async fn generate_blockchain(
                &mut self,
                chain_length: u64,
                starting_hash: SaitoHash,
            ) -> SaitoHash {
                let mut current_timestamp = create_timestamp();
                let mut parent_hash = starting_hash;

                {
                    if parent_hash != [0; 32] {
                        let blockchain = self.blockchain_lock.read().await;
                        let block_option = blockchain.get_block(&parent_hash).await;
                        let block = block_option.unwrap();
                        current_timestamp = block.get_timestamp() + 120000;
                    }
                }

                for i in 0..chain_length as u64 {
                    let mut vip_txs = 10;
                    if i > 0 {
                        vip_txs = 0;
                    }

                    let normal_txs = 0;
                    //let mut normal_txs = 1;
                    //if i == 0 { normal_txs = 0; }
                    //normal_txs = 0;

                    let mut has_gt = true;
                    if parent_hash == [0; 32] {
                        has_gt = false;
                    }
                    //if i%2 == 0 { has_gt = false; }

                    parent_hash = self
                        .add_block_on_hash(
                            current_timestamp + (i * 120000),
                            vip_txs,
                            normal_txs,
                            has_gt,
                            vec![],
                            parent_hash,
                        )
                        .await;
                }

                parent_hash
            }


            pub async fn generate_block_via_mempool(&self) -> Block {
                let latest_block_hash;
                let mut latest_block_timestamp = 0;

                let transaction = self.generate_transaction(1000, 1000).await;
                {
                    let mut mempool = self.mempool_lock.write().await;
                    mempool.add_transaction(transaction).await;
                }

                // get timestamp of previous block
                {
                    let blockchain = self.blockchain_lock.read().await;
                    latest_block_hash = blockchain.get_latest_block_hash();
                    if latest_block_hash != [0; 32] {
                        let block = blockchain.get_block(&latest_block_hash).await;
                        latest_block_timestamp = block.unwrap().get_timestamp();
                    }
                }

                let next_block_timestamp = latest_block_timestamp + (HEARTBEAT * 2);

                let block_option = self
                    .try_bundle_block(
                        self.mempool_lock.clone(),
                        self.blockchain_lock.clone(),
                        next_block_timestamp,
                    )
                    .await;

                assert!(block_option.is_some());
                block_option.unwrap()
            }
            async fn try_bundle_block(
                &self,
                mempool: Arc<RwLock<Mempool>>,
                blockchain: Arc<RwLock<Blockchain>>,
                timestamp: u64,
            ) -> Option<Block> {
                debug!("producing block...");
                let can_bundle: bool;
                {
                    let mempool = mempool.read().await;
                    can_bundle = mempool
                        .can_bundle_block(blockchain.clone(), timestamp)
                        .await;
                }

                if can_bundle {
                    let mempool = mempool.clone();
                    let mut mempool = mempool.write().await;
                    let result = mempool
                        .bundle_block(blockchain.clone().write().await.borrow_mut(), timestamp)
                        .await;
                    return Some(result);
                }
                return None;
            }


            pub fn check_block_consistency(block: &Block) {
                //
                // tests are run with blocks in various stages of
                // completion. in order to ensure that the tests here
                // can be comprehensive, we generate metadata if the
                // pre_hash has not been created.
                //
                let mut block2 = block.clone();
                block2.generate();

                let mut deserialized_block = Block::deserialize_from_net(&block2.serialize_for_net(BlockType::Full));

                deserialized_block.generate();

                assert_eq!(block2.get_id(), deserialized_block.get_id());
                assert_eq!(block2.get_timestamp(), deserialized_block.get_timestamp());
                assert_eq!(
                    block2.get_previous_block_hash(),
                    deserialized_block.get_previous_block_hash()
                );
                assert_eq!(block.get_creator(), deserialized_block.get_creator());
                assert_eq!(
                    block2.get_merkle_root(),
                    deserialized_block.get_merkle_root()
                );
                assert_eq!(block2.get_signature(), deserialized_block.get_signature());
                assert_eq!(block2.get_treasury(), deserialized_block.get_treasury());
                assert_eq!(block2.get_burnfee(), deserialized_block.get_burnfee());
                assert_eq!(block2.get_difficulty(), deserialized_block.get_difficulty());
                assert_eq!(
                    block2.get_staking_treasury(),
                    deserialized_block.get_staking_treasury()
                );
                // assert_eq!(block2.get_total_fees(), deserialized_block.get_total_fees());
                assert_eq!(
                    block2.get_routing_work_for_creator(),
                    deserialized_block.get_routing_work_for_creator()
                );
                // assert_eq!(block2.get_lc(), deserialized_block.get_lc());
                assert_eq!(
                    block2.get_has_golden_ticket(),
                    deserialized_block.get_has_golden_ticket()
                );
                assert_eq!(
                    block2.get_has_fee_transaction(),
                    deserialized_block.get_has_fee_transaction()
                );
                assert_eq!(
                    block2.get_golden_ticket_idx(),
                    deserialized_block.get_golden_ticket_idx()
                );
                assert_eq!(
                    block2.get_fee_transaction_idx(),
                    deserialized_block.get_fee_transaction_idx()
                );
                // assert_eq!(block2.get_total_rebroadcast_slips(), deserialized_block.get_total_rebroadcast_slips());
                // assert_eq!(block2.get_total_rebroadcast_nolan(), deserialized_block.get_total_rebroadcast_nolan());
                // assert_eq!(block2.get_rebroadcast_hash(), deserialized_block.get_rebroadcast_hash());
                //
                // in production blocks are required to have at least one transaction
                // but in testing we sometimes have blocks that do not have transactions
                // deserialization sets those blocks to Header blocks by default so we
                // only want to run this test if there are transactions in play
                //
                if block2.get_transactions().len() > 0 {
                    assert_eq!(block2.get_block_type(), deserialized_block.get_block_type());
                }
                assert_eq!(block2.get_pre_hash(), deserialized_block.get_pre_hash());
                assert_eq!(block2.get_hash(), deserialized_block.get_hash());

                let hashmap1 = &block2.slips_spent_this_block;
                let hashmap2 = &deserialized_block.slips_spent_this_block;

                //
                for (key, _value) in hashmap1 {
                    let value1 = hashmap1.get(key).unwrap();
                    let value2 = hashmap2.get(key).unwrap();
                    assert_eq!(value1, value2)
                }

                for (key, _value) in hashmap2 {
                    let value1 = hashmap1.get(key).unwrap();
                    let value2 = hashmap2.get(key).unwrap();
                    assert_eq!(value1, value2)
                }
            }

            pub fn set_latest_block_hash(&mut self, latest_block_hash: SaitoHash) {
                self.latest_block_hash = latest_block_hash;
            }

            pub fn spam_mempool(&mut self, mempool_lock: Arc<RwLock<Mempool>>) {
                let mempool_lock_clone = mempool_lock.clone();
                let wallet_lock_clone = self.wallet_lock.clone();
                let blockchain_lock_clone = self.blockchain_lock.clone();

                tokio::spawn(async move {
                    let txs_to_generate = 10;
                    let bytes_per_tx = 1024;
                    let public_key;
                    let private_key;
                    let latest_block_id;

                    {
                        let wallet = wallet_lock_clone.read().await;
                        public_key = wallet.get_public_key();
                        private_key = wallet.get_private_key();
                    }

                    {
                        let blockchain = blockchain_lock_clone.read().await;
                        latest_block_id = blockchain.get_latest_block_id();
                    }

                    {
                        if latest_block_id == 0 {
                            let mut vip_transaction = Transaction::generate_vip_transaction(
                                wallet_lock_clone.clone(),
                                public_key,
                                100_000_000,
                                10,
                            )
                            .await;
                            vip_transaction.sign(private_key);
                            let mut mempool = mempool_lock_clone.write().await;
                            mempool.add_transaction(vip_transaction).await;
                        }
                    }

                    loop {
                        for _i in 0..txs_to_generate {
                            let mut transaction = Transaction::generate_transaction(
                                wallet_lock_clone.clone(),
                                public_key,
                                5000,
                                5000,
                            )
                            .await;
                            transaction.set_message(
                                (0..bytes_per_tx)
                                    .into_iter()
                                    .map(|_| rand::random::<u8>())
                                    .collect(),
                            );
                            transaction.sign(private_key);
                            // before validation!
                            transaction.generate_metadata(public_key);

                            transaction
                                .add_hop_to_path(wallet_lock_clone.clone(), public_key)
                                .await;
                            transaction
                                .add_hop_to_path(wallet_lock_clone.clone(), public_key)
                                .await;
                            {
                                let mut mempool = mempool_lock_clone.write().await;
                                let blockchain = blockchain_lock_clone.read().await;
                                mempool
                                    .add_transaction_if_validates(transaction, &blockchain)
                                    .await;
                            }
                        }
                        sleep(Duration::from_millis(4000));
                        info!("TXS TO GENERATE: {:?}", txs_to_generate);
                    }
                });
            }
            pub async fn send_blocks_to_blockchain(
                &mut self,
                mempool_lock: Arc<RwLock<Mempool>>,
                blockchain_lock: Arc<RwLock<Blockchain>>,
            ) {
                let mut mempool = mempool_lock.write().await;
                // mempool.currently_bundling_block = true;
                let mut blockchain = blockchain_lock.write().await;
                while let Some(block) = mempool.blocks_queue.pop_front() {
                    mempool.delete_transactions(&block.get_transactions());
                    blockchain
                        .add_block(
                            block,
                            &mut self.network,
                            &mut self.storage,
                        )
                        .await;
                }
                // mempool.currently_bundling_block = false;
            }


        ***********************/
    }
}

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
use crate::test::test_io_handler::TestIOHandler;
use ahash::AHashMap;
use log::{debug, info, trace};
use rayon::prelude::*;
use saito_core::common::defs::{SaitoHash, SaitoPrivateKey, SaitoPublicKey, UtxoSet};

use saito_core::core::data::block::{Block, BlockType};
use saito_core::core::data::blockchain::Blockchain;
use saito_core::core::data::burnfee::HEARTBEAT;
use saito_core::core::data::crypto::{generate_random_bytes, hash};
use saito_core::core::data::golden_ticket::GoldenTicket;
use saito_core::core::data::mempool::Mempool;
use saito_core::core::data::network::Network;
use saito_core::core::data::peer_collection::PeerCollection;
use saito_core::core::data::storage::Storage;
use saito_core::core::data::transaction::{Transaction, TransactionType};
use saito_core::core::data::wallet::Wallet;
use saito_core::core::mining_event_processor::MiningEvent;
use std::borrow::BorrowMut;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread::sleep, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::RwLock;

pub fn create_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
        let peers = Arc::new(RwLock::new(PeerCollection::new()));
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let mempool_lock = Arc::new(RwLock::new(Mempool::new(wallet_lock.clone())));
        let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(10);

        Self {
            wallet_lock: wallet_lock,
            blockchain_lock: blockchain_lock,
            mempool_lock: mempool_lock,
            latest_block_hash: [0; 32],
            network: Network::new(Box::new(TestIOHandler::new()), peers.clone()),
            peers: peers.clone(),
            storage: Storage::new(Box::new(TestIOHandler::new())),
            sender_to_miner: sender_to_miner.clone(),
            receiver_in_miner,
        }
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
        let mut blockchain = self.blockchain_lock.write().await;
        blockchain
            .add_block(
                block,
                &mut self.network,
                &mut self.storage,
                self.sender_to_miner.clone(),
            )
            .await;
    }

    //
    // check that the blockchain connects properly
    //
    pub async fn check_blockchain(&self) {
        let blockchain = self.blockchain_lock.read().await;

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
                        block.unwrap().get_previous_block_hash(),
                        previous_block.unwrap().get_hash()
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

        info!("----");
        info!("----");
        info!("---- check utxoset ");
        info!("----");
        info!("----");
        for i in 1..=latest_block_id {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            info!("WINDING ID HASH - {} {:?}", i, block_hash);
            let block = blockchain.get_block(&block_hash).await.unwrap();
            for j in 0..block.get_transactions().len() {
                block.get_transactions()[j].on_chain_reorganization(&mut utxoset, true, i as u64);
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
        let mut block_fee_tx_idx: usize = 0;

        let blockchain = self.blockchain_lock.read().await;
        let latest_block_id = blockchain.get_latest_block_id();

        for i in 1..=latest_block_id {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i as u64);
            let block = blockchain.get_block(&block_hash).await.unwrap();

            block_inputs = 0;
            block_outputs = 0;
            block_contains_fee_tx = 0;

            previous_block_treasury = current_block_treasury;
            current_block_treasury = block.get_treasury();

            for t in 0..block.get_transactions().len() {
                //
                // we ignore the inputs in staking / fee transactions as they have
                // been pulled from the staking treasury and are already technically
                // counted in the money supply as an output from a previous slip.
                // we only care about the difference in token supply represented by
                // the difference in the staking_treasury.
                //
                if block.get_transactions()[t].get_transaction_type() == TransactionType::Fee {
                    block_contains_fee_tx = 1;
                    block_fee_tx_idx = t as usize;
                } else {
                    for z in 0..block.get_transactions()[t].inputs.len() {
                        block_inputs += block.get_transactions()[t].inputs[z].get_amount();
                    }
                    for z in 0..block.get_transactions()[t].outputs.len() {
                        block_outputs += block.get_transactions()[t].outputs[z].get_amount();
                    }
                }

                //
                // block one sets circulation
                //
                if i == 1 {
                    token_supply =
                        block_outputs + block.get_treasury() + block.get_staking_treasury();
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
                        let fee_transaction = &block.get_transactions()[block_fee_tx_idx];
                        for output in fee_transaction.get_outputs() {
                            total_fees_paid += output.get_amount();
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
                        + block.get_treasury()
                        + block.get_staking_treasury();

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
        let privatekey: SaitoPrivateKey;
        let publickey: SaitoPublicKey;

        {
            let wallet = self.wallet_lock.read().await;
            publickey = wallet.get_publickey();
            privatekey = wallet.get_privatekey();
        }

        for _i in 0..txs_number {
            let mut transaction =
                Transaction::create(self.wallet_lock.clone(), publickey, txs_amount, txs_fee).await;
            transaction.sign(privatekey);
            transaction.generate(publickey);
            transactions.push(transaction);
        }

        if include_valid_golden_ticket {
            let blockchain = self.blockchain_lock.read().await;
            let block = blockchain.get_block(&parent_hash).await.unwrap();
            let golden_ticket: GoldenTicket = Self::create_golden_ticket(
                self.wallet_lock.clone(),
                parent_hash,
                block.get_difficulty(),
            )
            .await;
            let mut gttx: Transaction;
            {
                let mut wallet = self.wallet_lock.write().await;
                gttx = wallet.create_golden_ticket_transaction(golden_ticket).await;
            }
            gttx.generate(publickey);
            transactions.push(gttx);
        }


        //
        // create block
        //
        let mut block = Block::create(
            &mut transactions,
            parent_hash,
            self.wallet_lock.clone(),
            self.blockchain_lock.clone().write().await.borrow_mut(),
            timestamp,
        )
        .await;
        block.generate();
        block.sign(privatekey);

        block
    }

    pub async fn create_golden_ticket(
        wallet: Arc<RwLock<Wallet>>,
        block_hash: SaitoHash,
        block_difficulty: u64,
    ) -> GoldenTicket {
        let public_key;
        {
            trace!("waiting for the wallet read lock");
            let wallet = wallet.read().await;
            trace!("acquired the wallet read lock");
            public_key = wallet.get_publickey();
        }
        let mut random_bytes = hash(&generate_random_bytes(32));

        let mut gt = GoldenTicket::create(block_hash, random_bytes, public_key);

        while !gt.validate(block_difficulty) {
            random_bytes = hash(&generate_random_bytes(32));
            gt = GoldenTicket::create(block_hash, random_bytes, public_key);
        }

        GoldenTicket::new(block_hash, random_bytes, public_key)
    }

    //
    // initialize chain
    //
    // creates and adds the first block to the blockchain, with however many VIP
    // transactions are necessary
    pub async fn initialize(&mut self, vip_transactions: u64, vip_amount: u64) {
        //
        // initialize timestamp
        //
        let timestamp = create_timestamp();

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
        let privatekey: SaitoPrivateKey;
        let publickey: SaitoPublicKey;
        {
            let wallet = self.wallet_lock.read().await;
            publickey = wallet.get_publickey();
            privatekey = wallet.get_privatekey();
        }

        //
        // create first block
        //
        let mut block = self.create_block([0; 32], timestamp, 0, 0, 0, false).await;

        //
        // generate UTXO-carrying VIP transactions
        //
        if 0 < vip_transactions {
            let mut tx = Transaction::create_vip_transaction(publickey, vip_amount);
            tx.generate(publickey);
            tx.sign(privatekey);
            block.add_transaction(tx);
        }

        //
        // we have added VIP, so need to regenerate the merkle-root
        //
        block.set_merkle_root(block.generate_merkle_root());
        block.generate();
        block.sign(privatekey);

        //
        // and add first block to blockchain
        //
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

            let privatekey: SaitoPrivateKey;
            let publickey: SaitoPublicKey;

            {
                let wallet = self.wallet_lock.read().await;
                publickey = wallet.get_publickey();
                privatekey = wallet.get_privatekey();
            }
            block.sign(privatekey);
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
                let publickey;
                let privatekey;
                let latest_block_id;

                {
                    let wallet = wallet_lock_clone.read().await;
                    publickey = wallet.get_publickey();
                    privatekey = wallet.get_privatekey();
                }

                {
                    let blockchain = blockchain_lock_clone.read().await;
                    latest_block_id = blockchain.get_latest_block_id();
                }

                {
                    if latest_block_id == 0 {
                        let mut vip_transaction = Transaction::generate_vip_transaction(
                            wallet_lock_clone.clone(),
                            publickey,
                            100_000_000,
                            10,
                        )
                        .await;
                        vip_transaction.sign(privatekey);
                        let mut mempool = mempool_lock_clone.write().await;
                        mempool.add_transaction(vip_transaction).await;
                    }
                }

                loop {
                    for _i in 0..txs_to_generate {
                        let mut transaction = Transaction::generate_transaction(
                            wallet_lock_clone.clone(),
                            publickey,
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
                        transaction.sign(privatekey);
                        // before validation!
                        transaction.generate_metadata(publickey);

                        transaction
                            .add_hop_to_path(wallet_lock_clone.clone(), publickey)
                            .await;
                        transaction
                            .add_hop_to_path(wallet_lock_clone.clone(), publickey)
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

#[cfg(test)]
mod tests {}

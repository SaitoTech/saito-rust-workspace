#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use log::info;
    use tokio::sync::RwLock;

    use saito_core::core::data::blockchain::Blockchain;
    use saito_core::core::data::wallet::Wallet;

    use crate::test::test_manager::{create_timestamp, TestManager};

    #[tokio::test]
    #[serial_test::serial]
    async fn initialize_blockchain_test() {
        let mut t = TestManager::new();

        // create first block, with 100 VIP txs with 1_000_000_000 NOLAN each
        t.initialize(100, 1_000_000_000).await;
        t.wait_for_mining_event().await;

        let blockchain = t.blockchain_lock.read().await;
        assert_eq!(1, blockchain.get_latest_block_id());

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    #[tokio::test]
    #[serial_test::serial]
    //
    // test we can produce five blocks in a row
    //
    async fn add_five_good_blocks() {
        let mut t = TestManager::new();
        let mut block1;
        let mut block1_id;
        let mut block1_hash;
        let mut ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.get_id();
            block1_hash = block1.get_hash();
            ts = block1.get_timestamp();

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.get_hash();
        let block2_id = block2.get_id();

        t.add_block(block2).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.get_hash();
        let block3_id = block3.get_id();

        t.add_block(block3).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.get_hash();
        let block4_id = block4.get_id();

        t.add_block(block4).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.get_hash();
        let block5_id = block5.get_id();

        t.add_block(block5).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    #[tokio::test]
    #[serial_test::serial]
    //
    // test we do not add blocks because of insufficient mining
    //
    async fn insufficient_golden_tickets_test() {

        let mut t = TestManager::new();
        let mut block1;
        let mut block1_id;
        let mut block1_hash;
        let mut ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.get_id();
            block1_hash = block1.get_hash();
            ts = block1.get_timestamp();

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.get_hash();
        let block2_id = block2.get_id();

        t.add_block(block2).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.get_hash();
        let block3_id = block3.get_id();

println!("HASH OF BLOCK THREE IS: {:?}", block3.get_hash());

        t.add_block(block3).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.get_hash();
        let block4_id = block4.get_id();

println!("HASH OF BLOCK FOUR IS: {:?}", block4.get_hash());

        t.add_block(block4).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.get_hash();
        let block5_id = block5.get_id();

println!("HASH OF BLOCK FIVE IS: {:?}", block5.get_hash());

        t.add_block(block5).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        //
        // block 6
        //
        let mut block6 = t
            .create_block(
                block5_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block6.generate(); // generate hashes

println!("ID OF BLOCK SIX IS: {}", block6.get_id());
println!("HASH OF BLOCK SIX IS: {:?}", block6.get_hash());

        let block6_hash = block6.get_hash();
        let block6_id = block6.get_id();

        t.add_block(block6).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_ne!(blockchain.get_latest_block_hash(), block6_hash);
            assert_ne!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }


        //
        // block 7
        //
        let mut block7 = t
            .create_block(
                block6_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block7.generate(); // generate hashes

        let block7_hash = block7.get_hash();
        let block7_id = block7.get_id();

        t.add_block(block7).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_ne!(blockchain.get_latest_block_hash(), block6_hash);
            assert_ne!(blockchain.get_latest_block_id(), block6_id);
            assert_ne!(blockchain.get_latest_block_hash(), block7_hash);
            assert_ne!(blockchain.get_latest_block_id(), block7_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }


        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }


    #[tokio::test]
    #[serial_test::serial]
    //
    // test we do not add blocks because of insufficient mining
    //
    async fn seven_blocks_with_sufficient_golden_tickets_test() {

        let mut t = TestManager::new();
        let mut block1;
        let mut block1_id;
        let mut block1_hash;
        let mut ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.get_id();
            block1_hash = block1.get_hash();
            ts = block1.get_timestamp();

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.get_hash();
        let block2_id = block2.get_id();

        t.add_block(block2).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.get_hash();
        let block3_id = block3.get_id();

        t.add_block(block3).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.get_hash();
        let block4_id = block4.get_id();

        t.add_block(block4).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.get_hash();
        let block5_id = block5.get_id();

        t.add_block(block5).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        //
        // block 6
        //
        let mut block6 = t
            .create_block(
                block5_hash, // hash of parent block
                ts + 600000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block6.generate(); // generate hashes

        let block6_hash = block6.get_hash();
        let block6_id = block6.get_id();

        t.add_block(block6).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(blockchain.get_latest_block_hash(), block5_hash);
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_hash(), block6_hash);
            assert_eq!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_id(), 6);
        }


        //
        // block 7
        //
        let mut block7 = t
            .create_block(
                block6_hash, // hash of parent block
                ts + 720000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,        // mine golden ticket
            )
            .await;
        block7.generate(); // generate hashes

        let block7_hash = block7.get_hash();
        let block7_id = block7.get_id();

        t.add_block(block7).await;

        {
            let blockchain = t.blockchain_lock.write().await;
            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(blockchain.get_latest_block_hash(), block5_hash);
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_ne!(blockchain.get_latest_block_hash(), block6_hash);
            assert_ne!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_hash(), block7_hash);
            assert_eq!(blockchain.get_latest_block_id(), block7_id);
            assert_eq!(blockchain.get_latest_block_id(), 7);
        }


        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }









/**************************
        #[tokio::test]
        #[serial_test::serial]
        //
        // test we add blocks 6 and 7 because of suffient mining
        //
        async fn add_seven_good_blocks_and_two_golden_tickets() {
            TestManager::clear_data_folder().await;
            let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
            let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
            let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(10);
            let mut test_manager = TestManager::new(
                blockchain_lock.clone(),
                wallet_lock.clone(),
                sender_miner.clone(),
            );

            let current_timestamp = create_timestamp();

            // BLOCK 1
            test_manager
                .add_block(current_timestamp, 3, 0, false, vec![])
                .await;

            // BLOCK 2
            test_manager
                .add_block(current_timestamp + 120000, 0, 1, false, vec![])
                .await;

            // BLOCK 3
            test_manager
                .add_block(current_timestamp + 240000, 0, 1, true, vec![])
                .await;

            // BLOCK 4
            test_manager
                .add_block(current_timestamp + 360000, 0, 1, false, vec![])
                .await;

            // BLOCK 5
            test_manager
                .add_block(current_timestamp + 480000, 0, 1, false, vec![])
                .await;

            // BLOCK 6
            test_manager
                .add_block(current_timestamp + 600000, 0, 1, true, vec![])
                .await;

            // BLOCK 7
            test_manager
                .add_block(current_timestamp + 720000, 0, 1, false, vec![])
                .await;

            let blockchain = blockchain_lock.read().await;

            assert_eq!(7, blockchain.get_latest_block_id());
            assert_ne!(5, blockchain.get_latest_block_id());

            test_manager.check_utxoset().await;
            test_manager.check_token_supply().await;
        }

        #[tokio::test]
        #[serial_test::serial]
        //
        // add 10 blocks including chain reorganization and sufficient mining
        //
        async fn add_ten_blocks_including_five_block_chain_reorg() {
            TestManager::clear_data_folder().await;
            let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
            let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
            let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(10);
            let mut test_manager = TestManager::new(
                blockchain_lock.clone(),
                wallet_lock.clone(),
                sender_miner.clone(),
            );

            let current_timestamp = create_timestamp();
            let block5_hash;
            let block9_hash;

            // BLOCK 1
            test_manager
                .add_block(current_timestamp, 3, 0, false, vec![])
                .await;

            // BLOCK 2
            test_manager
                .add_block(current_timestamp + 120000, 0, 1, false, vec![])
                .await;

            // BLOCK 3
            test_manager
                .add_block(current_timestamp + 240000, 0, 1, true, vec![])
                .await;

            // BLOCK 4
            test_manager
                .add_block(current_timestamp + 360000, 0, 1, false, vec![])
                .await;

            // BLOCK 5
            // note - we set GT to true so that the staking payout won't regress
            // since the test_manager produces from known state of wallet rather
            // that the state at block 5.
            test_manager
                .add_block(current_timestamp + 480000, 0, 1, true, vec![])
                .await;

            {
                // check that block 5 is indeed the latest block
                let blockchain = blockchain_lock.read().await;
                assert_eq!(5, blockchain.get_latest_block_id());
                block5_hash = blockchain.get_latest_block_hash();
            }

            //
            // produce the next four blocks, which we will reorg eventually
            //

            // BLOCK 6
            test_manager
                .add_block(current_timestamp + 600000, 0, 1, true, vec![])
                .await;

            // BLOCK 7
            test_manager
                .add_block(current_timestamp + 720000, 0, 1, false, vec![])
                .await;

            // BLOCK 8
            test_manager
                .add_block(current_timestamp + 840000, 0, 1, true, vec![])
                .await;

            // BLOCK 9
            test_manager
                .add_block(current_timestamp + 960000, 0, 1, false, vec![])
                .await;

            {
                let blockchain = blockchain_lock.read().await;
                // check that block 9 is indeed the latest block
                assert_eq!(9, blockchain.get_latest_block_id());
                block9_hash = blockchain.get_latest_block_hash();
            }

            //
            // start a separate fork that reorgs from block 5
            //
            // note that we have to run generate_metadata, as we cannot
            // calculate some dynamic variables like the appropriate
            // difficulty without being able to check if the previous
            // block has golden tickets, etc. which is calculated in
            // generate_metadata(). this is not a problem in production
            // as blocks are built off the longest-chain.
            //

            // BLOCK 6-2
            let block6_2 = test_manager
                .generate_block_and_metadata(
                    block5_hash,
                    current_timestamp + 600000,
                    0,
                    0,
                    true,
                    vec![],
                )
                .await;
            let block6_2_hash = block6_2.get_hash();
            TestManager::add_block_to_blockchain(
                blockchain_lock.clone(),
                block6_2,
                &mut test_manager.network,
                &mut test_manager.storage,
                test_manager.sender_to_miner.clone(),
            )
            .await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(9, blockchain.get_latest_block_id());
                assert_eq!(block9_hash, blockchain.get_latest_block_hash());
            }

            // BLOCK 7-2
            let block7_2 = test_manager
                .generate_block_and_metadata(
                    block6_2_hash,
                    current_timestamp + 720000,
                    0,
                    0,
                    true,
                    vec![],
                )
                .await;
            let block7_2_hash = block7_2.get_hash();
            TestManager::add_block_to_blockchain(
                blockchain_lock.clone(),
                block7_2,
                &mut test_manager.network,
                &mut test_manager.storage,
                test_manager.sender_to_miner.clone(),
            )
            .await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(9, blockchain.get_latest_block_id());
                assert_eq!(block9_hash, blockchain.get_latest_block_hash());
            }

            // BLOCK 8-2
            let block8_2 = test_manager
                .generate_block_and_metadata(
                    block7_2_hash,
                    current_timestamp + 840000,
                    0,
                    0,
                    true,
                    vec![],
                )
                .await;
            let block8_2_hash = block8_2.get_hash();
            TestManager::add_block_to_blockchain(
                blockchain_lock.clone(),
                block8_2,
                &mut test_manager.network,
                &mut test_manager.storage,
                test_manager.sender_to_miner.clone(),
            )
            .await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(9, blockchain.get_latest_block_id());
                assert_eq!(block9_hash, blockchain.get_latest_block_hash());
            }

            // BLOCK 9-2
            let block9_2 = test_manager
                .generate_block_and_metadata(
                    block8_2_hash,
                    current_timestamp + 960000,
                    0,
                    0,
                    true,
                    vec![],
                )
                .await;
            let block9_2_hash = block9_2.get_hash();
            TestManager::add_block_to_blockchain(
                blockchain_lock.clone(),
                block9_2,
                &mut test_manager.network,
                &mut test_manager.storage,
                test_manager.sender_to_miner.clone(),
            )
            .await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(9, blockchain.get_latest_block_id());
                assert_eq!(block9_hash, blockchain.get_latest_block_hash());
            }

            // BLOCK 10-2
            let block10_2 = test_manager
                .generate_block_and_metadata(
                    block9_2_hash,
                    current_timestamp + 1080000,
                    0,
                    0,
                    true,
                    vec![],
                )
                .await;
            let block10_2_hash = block10_2.get_hash();
            TestManager::add_block_to_blockchain(
                blockchain_lock.clone(),
                block10_2,
                &mut test_manager.network,
                &mut test_manager.storage,
                test_manager.sender_to_miner.clone(),
            )
            .await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(10, blockchain.get_latest_block_id());
                assert_eq!(block10_2_hash, blockchain.get_latest_block_hash());
            }

            test_manager.check_utxoset().await;
            test_manager.check_token_supply().await;
        }

        #[tokio::test]
        #[serial_test::serial]
        //
        // use test_manager to generate blockchains and reorgs and test
        //
        async fn test_manager_blockchain_fork_test() {
            TestManager::clear_data_folder().await;
            let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
            let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
            let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(1000);
            let mut test_manager = TestManager::new(
                blockchain_lock.clone(),
                wallet_lock.clone(),
                sender_miner.clone(),
            );
            // 5 initial blocks
            test_manager.generate_blockchain(5, [0; 32]).await;

            let block5_hash;

            {
                let blockchain = blockchain_lock.read().await;
                block5_hash = blockchain.get_latest_block_hash();

                assert_eq!(
                    blockchain
                        .blockring
                        .get_longest_chain_block_hash_by_block_id(5),
                    block5_hash
                );
                assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            }

            // 5 block reorg with 10 block fork
            let block10_hash = test_manager.generate_blockchain(5, block5_hash).await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(
                    blockchain
                        .blockring
                        .get_longest_chain_block_hash_by_block_id(10),
                    block10_hash
                );
                assert_eq!(blockchain.get_latest_block_hash(), block10_hash);
            }

            let block15_hash = test_manager.generate_blockchain(10, block5_hash).await;

            {
                let blockchain = blockchain_lock.read().await;
                assert_eq!(
                    blockchain
                        .blockring
                        .get_longest_chain_block_hash_by_block_id(15),
                    block15_hash
                );
                assert_eq!(blockchain.get_latest_block_hash(), block15_hash);
            }
        }

        /// Loading blocks into a blockchain which was were created from another blockchain instance
        #[tokio::test]
        #[serial_test::serial]
        async fn load_blocks_from_another_blockchain_test() {
            info!("current dir = {:?}", std::env::current_dir().unwrap());
            TestManager::clear_data_folder().await;
            let wallet_lock1 = Arc::new(RwLock::new(Wallet::new()));
            let blockchain_lock1 = Arc::new(RwLock::new(Blockchain::new(wallet_lock1.clone())));
            let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(10);
            let mut test_manager1 = TestManager::new(
                blockchain_lock1.clone(),
                wallet_lock1.clone(),
                sender_miner.clone(),
            );

            let current_timestamp = create_timestamp();

            let block1_hash = test_manager1
                .add_block(current_timestamp + 100000, 0, 10, false, vec![])
                .await;
            let block2_hash = test_manager1
                .add_block(current_timestamp + 200000, 0, 20, true, vec![])
                .await;

            let wallet_lock2 = Arc::new(RwLock::new(Wallet::new()));
            let blockchain_lock2 = Arc::new(RwLock::new(Blockchain::new(wallet_lock2.clone())));
            let mut test_manager2 = TestManager::new(
                blockchain_lock2.clone(),
                wallet_lock2.clone(),
                sender_miner.clone(),
            );

            test_manager2
                .storage
                .load_blocks_from_disk(
                    blockchain_lock2.clone(),
                    &mut test_manager2.network,
                    test_manager2.sender_to_miner.clone(),
                )
                .await;

            {
                let blockchain1 = blockchain_lock1.read().await;
                let blockchain2 = blockchain_lock2.read().await;

                let block1_chain1 = blockchain1.get_block(&block1_hash).await.unwrap();
                let block1_chain2 = blockchain2.get_block(&block1_hash).await.unwrap();

                let block2_chain1 = blockchain1.get_block(&block2_hash).await.unwrap();
                let block2_chain2 = blockchain2.get_block(&block2_hash).await.unwrap();

                for (block_new, block_old) in &[
                    (block1_chain2, block1_chain1),
                    (block2_chain2, block2_chain1),
                ] {
                    assert_eq!(block_new.get_hash(), block_old.get_hash());
                    assert_eq!(block_new.has_golden_ticket, block_old.has_golden_ticket);
                    assert_eq!(
                        block_new.get_previous_block_hash(),
                        block_old.get_previous_block_hash()
                    );
                    assert_eq!(block_new.get_block_type(), block_old.get_block_type());
                    assert_eq!(block_new.get_signature(), block_old.get_signature());
                }
            }
        }
**************/
}

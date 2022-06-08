#[cfg(test)]
mod tests {
    use crate::test::test_manager::{create_timestamp, TestManager};

    use log::info;
    use saito_core::core::data::blockchain::Blockchain;

    use saito_core::core::data::wallet::Wallet;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    #[serial_test::serial]
    async fn initialize_blockchain_test() {

        let timestamp = create_timestamp();

	let mut t = TestManager::new();
	t.initialize(
	    timestamp,
	    100,
	    1_000_000_000,
	);

	let blockchain = t.blockchain_lock.read().await;

	assert_eq!(1, blockchain.get_latest_block_id());

    }


/*******
    #[tokio::test]
    #[serial_test::serial]
    //
    // test we can produce five blocks in a row
    //
    async fn add_five_good_blocks() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );


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
            .add_block(current_timestamp + 240000, 0, 1, false, vec![])
            .await;

        // BLOCK 4
        test_manager
            .add_block(current_timestamp + 360000, 0, 1, false, vec![])
            .await;

        // BLOCK 5
        test_manager
            .add_block(current_timestamp + 480000, 0, 1, false, vec![])
            .await;

        let blockchain = blockchain_lock.read().await;

        assert_eq!(5, blockchain.get_latest_block_id());

        test_manager.check_utxoset().await;
        test_manager.check_token_supply().await;
    }
    #[tokio::test]
    #[serial_test::serial]
    //
    // test we do not add blocks 6 and 7 because of insuffient mining
    //
    async fn add_seven_good_blocks_but_no_golden_tickets() {
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
            .add_block(current_timestamp + 240000, 0, 1, false, vec![])
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
            .add_block(current_timestamp + 600000, 0, 1, false, vec![])
            .await;

        // BLOCK 7
        test_manager
            .add_block(current_timestamp + 720000, 0, 1, false, vec![])
            .await;

        let blockchain = blockchain_lock.read().await;

        assert_eq!(5, blockchain.get_latest_block_id());
        assert_ne!(7, blockchain.get_latest_block_id());

        test_manager.check_utxoset().await;
        test_manager.check_token_supply().await;
    }

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
*****/
}

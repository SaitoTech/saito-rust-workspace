#[cfg(test)]
mod test {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use saito_core::core::data::blockchain::Blockchain;
    use saito_core::core::data::blockring::BlockRing;
    use saito_core::core::data::wallet::Wallet;

    use crate::test::test_manager::{create_timestamp, TestManager};
    use crate::IoEvent;

    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    // winding / unwinding updates blockring view of longest chain
    async fn blockring_manual_reorganization_test() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );
        let mut blockring = BlockRing::new();

        let current_timestamp = create_timestamp();

        // BLOCK 1
        let mut block1 = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 3, 0, false, vec![])
            .await;
        block1.set_id(1);

        // BLOCK 2
        let mut block2 = test_manager
            .generate_block_and_metadata(
                block1.get_hash(),
                current_timestamp + 120000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        block2.set_id(2);

        // BLOCK 3
        let mut block3 = test_manager
            .generate_block_and_metadata(
                block2.get_hash(),
                current_timestamp + 240000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        block3.set_id(3);

        // BLOCK 4
        let mut block4 = test_manager
            .generate_block_and_metadata(
                block3.get_hash(),
                current_timestamp + 360000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        block4.set_id(4);

        // BLOCK 5
        let mut block5 = test_manager
            .generate_block_and_metadata(
                block4.get_hash(),
                current_timestamp + 480000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        block5.set_id(5);

        blockring.add_block(&block1);
        blockring.add_block(&block2);
        blockring.add_block(&block3);
        blockring.add_block(&block4);
        blockring.add_block(&block5);

        // do we contain these block hashes?
        assert_eq!(
            blockring.contains_block_hash_at_block_id(1, block1.get_hash()),
            true
        );
        assert_eq!(
            blockring.contains_block_hash_at_block_id(2, block2.get_hash()),
            true
        );
        assert_eq!(
            blockring.contains_block_hash_at_block_id(3, block3.get_hash()),
            true
        );
        assert_eq!(
            blockring.contains_block_hash_at_block_id(4, block4.get_hash()),
            true
        );
        assert_eq!(
            blockring.contains_block_hash_at_block_id(5, block5.get_hash()),
            true
        );
        assert_eq!(
            blockring.contains_block_hash_at_block_id(2, block4.get_hash()),
            false
        );

        // reorganize longest chain
        blockring.on_chain_reorganization(1, block1.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 1);
        blockring.on_chain_reorganization(2, block2.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 2);
        blockring.on_chain_reorganization(3, block3.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 3);
        blockring.on_chain_reorganization(4, block4.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 4);
        blockring.on_chain_reorganization(5, block5.get_hash(), false);
        assert_eq!(blockring.get_latest_block_id(), 4);
        blockring.on_chain_reorganization(4, block4.get_hash(), false);
        assert_eq!(blockring.get_latest_block_id(), 3);
        blockring.on_chain_reorganization(3, block3.get_hash(), false);
        assert_eq!(blockring.get_latest_block_id(), 2);

        // reorg in the wrong block_id location, should not change
        blockring.on_chain_reorganization(532, block5.get_hash(), false);
        assert_eq!(blockring.get_latest_block_id(), 2);

        // double reorg in correct and should be fine still
        blockring.on_chain_reorganization(2, block2.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 2);
    }

    #[tokio::test]
    #[serial_test::serial]
    // adding blocks to blockchain wind / unwind blockring view of longest chain
    async fn blockring_automatic_reorganization_test() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        let current_timestamp = create_timestamp();

        // BLOCK 1
        let block1 = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 3, 0, false, vec![])
            .await;
        let block1_hash = block1.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block1,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        // BLOCK 2
        let block2 = test_manager
            .generate_block_and_metadata(
                block1_hash,
                current_timestamp + 120000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        let block2_hash = block2.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block2,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        // BLOCK 3
        let block3 = test_manager
            .generate_block_and_metadata(
                block2_hash,
                current_timestamp + 240000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        let block3_hash = block3.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block3,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        // BLOCK 4
        let block4 = test_manager
            .generate_block_and_metadata(
                block3_hash,
                current_timestamp + 360000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        let block4_hash = block4.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block4,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        // BLOCK 5
        let block5 = test_manager
            .generate_block_and_metadata(
                block4_hash,
                current_timestamp + 480000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        let block5_hash = block5.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block5,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        let mut blockchain = blockchain_lock.write().await;

        // do we contain these block hashes?
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(1, block1_hash),
            true
        );
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(2, block2_hash),
            true
        );
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(3, block3_hash),
            true
        );
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(4, block4_hash),
            true
        );
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(5, block5_hash),
            true
        );
        assert_eq!(
            blockchain
                .blockring
                .contains_block_hash_at_block_id(2, block4_hash),
            false
        );

        // reorganize longest chain
        assert_ne!(blockchain.blockring.get_latest_block_id(), 1);
        assert_ne!(blockchain.blockring.get_latest_block_id(), 2);
        assert_ne!(blockchain.blockring.get_latest_block_id(), 3);
        assert_ne!(blockchain.blockring.get_latest_block_id(), 4);
        assert_eq!(blockchain.blockring.get_latest_block_id(), 5);
        assert_eq!(blockchain.blockring.get_latest_block_hash(), block5_hash);

        // reorg in the wrong block_id location, should not change
        blockchain
            .blockring
            .on_chain_reorganization(532, block5_hash, false);
        assert_ne!(blockchain.blockring.get_latest_block_id(), 2);

        blockchain
            .blockring
            .on_chain_reorganization(5, block5_hash, false);
        assert_eq!(blockchain.blockring.get_latest_block_id(), 4);
        assert_eq!(blockchain.blockring.get_latest_block_hash(), block4_hash);
    }

    #[tokio::test]
    #[serial_test::serial]
    // confirm adding block changes nothing until on_chain_reorg
    async fn blockring_add_block_test() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        let current_timestamp = create_timestamp();

        let mut blockring = BlockRing::new();
        assert_eq!(0, blockring.get_latest_block_id());

        // BLOCK 1
        let block1 = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 3, 0, false, vec![])
            .await;
        let block1_hash = block1.get_hash();
        blockring.add_block(&block1);
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block1,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        // BLOCK 2
        let block2 = test_manager
            .generate_block_and_metadata(
                block1_hash,
                current_timestamp + 120000,
                0,
                1,
                false,
                vec![],
            )
            .await;
        let block2_hash = block2.get_hash();
        blockring.add_block(&block2);
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block2,
            &mut test_manager.network,
            &mut test_manager.storage,
            test_manager.sender_to_miner.clone(),
        )
        .await;

        assert_eq!(0, blockring.get_latest_block_id());
        assert_eq!([0; 32], blockring.get_latest_block_hash());
        assert_eq!(
            [0; 32],
            blockring.get_longest_chain_block_hash_by_block_id(0)
        );

        blockring.on_chain_reorganization(1, block1_hash, true);

        assert_eq!(1, blockring.get_latest_block_id());
        assert_eq!(block1_hash, blockring.get_latest_block_hash());
        assert_eq!(
            [0; 32],
            blockring.get_longest_chain_block_hash_by_block_id(0)
        );

        blockring.on_chain_reorganization(1, block1_hash, false);

        assert_eq!(0, blockring.get_latest_block_id());
        assert_eq!([0; 32], blockring.get_latest_block_hash());
        assert_eq!(
            [0; 32],
            blockring.get_longest_chain_block_hash_by_block_id(0)
        );

        blockring.on_chain_reorganization(1, block1_hash, true);
        blockring.on_chain_reorganization(2, block2_hash, true);

        assert_eq!(2, blockring.get_latest_block_id());
        assert_eq!(block2_hash, blockring.get_latest_block_hash());
        assert_eq!(
            [0; 32],
            blockring.get_longest_chain_block_hash_by_block_id(3)
        );
    }
}

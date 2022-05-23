#[cfg(test)]
mod tests {
    use crate::test::test_manager::{create_timestamp, TestManager};
    use saito_core::core::data::blockchain::Blockchain;

    use saito_core::core::data::wallet::Wallet;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    #[serial_test::serial]
    async fn mempool_bundle_blocks_test() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, _receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );
        let mempool_lock = test_manager.mempool_lock.clone();

        // BLOCK 1 - VIP transactions
        test_manager
            .add_block(create_timestamp(), 3, 0, false, vec![])
            .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        for _i in 0..4 {
            let block = test_manager.generate_block_via_mempool().await;
            {
                let mut mempool = test_manager.mempool_lock.write().await;
                mempool.add_block(block);
            }
            test_manager
                .send_blocks_to_blockchain(mempool_lock.clone(), blockchain_lock.clone())
                .await;
        }

        // check chain consistence
        test_manager.check_blockchain().await;
    }
}

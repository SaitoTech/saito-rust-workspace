#[cfg(test)]
mod tests {
    use crate::test::test_manager::{create_timestamp, TestManager};
    use saito_core::core::data::blockchain::{Blockchain, MAX_TOKEN_SUPPLY};
    use saito_core::core::data::storage::Storage;
    use saito_core::core::data::wallet::Wallet;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // impl Drop for Blockchain {
    //     fn drop(&mut self) {
    //         let paths: Vec<_> = fs::read_dir(BLOCKS_DIR_PATH.clone())
    //             .unwrap()
    //             .map(|r| r.unwrap())
    //             .collect();
    //         for (_pos, path) in paths.iter().enumerate() {
    //             if !path.path().to_str().unwrap().ends_with(".gitignore") {
    //                 match std::fs::remove_file(path.path()) {
    //                     Err(err) => {
    //                         eprintln!("Error cleaning up after tests {}", err);
    //                     }
    //                     _ => {
    //                         log::trace!(
    //                             "block removed from disk : {}",
    //                             path.path().to_str().unwrap()
    //                         );
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    #[test]
    fn read_issuance_file_test() {
        let slips = Storage::return_token_supply_slips_from_disk();
        let mut total_issuance = 0;

        for i in 0..slips.len() {
            total_issuance += slips[i].get_amount();
        }

        assert_eq!(total_issuance, MAX_TOKEN_SUPPLY);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn write_read_block_to_file_test() {
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        let current_timestamp = create_timestamp();

        let mut block = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 0, 1, false, vec![])
            .await;

        let filename = Storage::write_block_to_disk(&mut block, &mut test_manager.io_handler).await;
        log::trace!("block written to file : {}", filename);
        let retrieved_block =
            Storage::load_block_from_disk(filename, &mut test_manager.io_handler).await;

        assert!(retrieved_block.is_ok());
        assert_eq!(block.get_hash(), retrieved_block.unwrap().get_hash());
    }
}

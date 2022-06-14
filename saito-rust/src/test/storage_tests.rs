#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use saito_core::core::data::block::Block;
    use saito_core::core::data::blockchain::{Blockchain, MAX_TOKEN_SUPPLY};
    use saito_core::core::data::wallet::Wallet;
    use crate::test::test_manager::{create_timestamp, TestManager};


    #[ignore]
    #[tokio::test]
    async fn read_issuance_file_test() {

        let mut t = TestManager::new();
	t.initialize(100, 100_000_000).await;

        let slips = t.storage.return_token_supply_slips_from_disk();
        let mut total_issuance = 0;

        for i in 0..slips.len() {
            total_issuance += slips[i].get_amount();
        }

        assert_eq!(total_issuance, MAX_TOKEN_SUPPLY);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn write_read_block_to_file_test() {


        let mut t = TestManager::new();
	t.initialize(100, 100_000_000);

        let current_timestamp = create_timestamp();

        let mut block = Block::new();
	block.set_timestamp(current_timestamp);

        let filename = t.storage.write_block_to_disk(&mut block).await;
        log::trace!("block written to file : {}", filename);
        let retrieved_block = t.storage.load_block_from_disk(filename).await;
	let mut actual_retrieved_block = retrieved_block.unwrap();
	actual_retrieved_block.generate();

        assert_eq!(block.get_timestamp(), actual_retrieved_block.get_timestamp());
    }

}

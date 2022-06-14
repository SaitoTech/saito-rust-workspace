#[cfg(test)]
mod test {
    use log::info;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use saito_core::core::data::block::Block;
    use saito_core::core::data::blockchain::Blockchain;
    use saito_core::core::data::blockring::BlockRing;
    use saito_core::core::data::wallet::Wallet;

    use crate::test::test_manager::{create_timestamp, TestManager};

    #[tokio::test]
    #[serial_test::serial]
    //
    // does reorg update blockring view of longest-chain
    //
    async fn blockring_manual_reorganization_test() {
        let mut block1 = Block::new();
        let mut block2 = Block::new();
        let mut block3 = Block::new();
        let mut block4 = Block::new();
        let mut block5 = Block::new();

        block1.set_id(1);
        block2.set_id(2);
        block3.set_id(3);
        block4.set_id(4);
        block5.set_id(5);

        block1.generate();
        block2.generate();
        block3.generate();
        block4.generate();
        block5.generate();

        let mut blockring = BlockRing::new();

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
        blockring.on_chain_reorganization(5, block5.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 5);
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
        blockring.on_chain_reorganization(2, block2.get_hash(), true);
        assert_eq!(blockring.get_latest_block_id(), 2);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::future::join_all;
    use tokio::sync::RwLock;

    use saito_core::core::data::block::{Block, BlockType};
    use saito_core::core::data::blockchain::Blockchain;

    use saito_core::core::data::transaction::Transaction;
    use saito_core::core::data::wallet::Wallet;

    use crate::test::test_manager::TestManager;

    #[tokio::test]
    #[serial_test::serial]
    // downgrade and upgrade a block with transactions
    async fn block_downgrade_upgrade_test() {
        let mut t = TestManager::new();
        let mut wallet_lock = t.wallet_lock.clone();
        let mut block = Block::new();
        let mut transactions = join_all((0..5).into_iter().map(|_| async {
            let mut transaction = Transaction::new();
            let wallet = wallet_lock.read().await;
            transaction.sign(wallet.get_privatekey());
            transaction
        }))
        .await
        .to_vec();

        block.transactions = transactions;
        block.generate();

        // save to disk
        t.storage.write_block_to_disk(&mut block).await;

        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.get_block_type(), BlockType::Full);

        let serialized_full_block = block.serialize_for_net(BlockType::Full);
        block
            .update_block_to_block_type(BlockType::Pruned, &mut t.storage)
            .await;

        assert_eq!(block.transactions.len(), 0);
        assert_eq!(block.get_block_type(), BlockType::Pruned);

        block
            .update_block_to_block_type(BlockType::Full, &mut t.storage)
            .await;

        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.get_block_type(), BlockType::Full);
        assert_eq!(
            serialized_full_block,
            block.serialize_for_net(BlockType::Full)
        );
    }
}

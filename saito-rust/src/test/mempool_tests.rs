#[cfg(test)]
mod tests {
    use crate::test::test_manager::{create_timestamp, TestManager};
    use saito_core::common::defs::{SaitoPrivateKey, SaitoPublicKey};
    use saito_core::core::data::blockchain::Blockchain;
    use saito_core::core::data::burnfee::HEARTBEAT;
    use saito_core::core::data::mempool::Mempool;
    use saito_core::core::data::transaction::Transaction;

    use saito_core::core::data::wallet::Wallet;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    #[serial_test::serial]
    async fn mempool_bundle_blocks_test() {
        let mempool_lock: Arc<RwLock<Mempool>>;
        let wallet_lock: Arc<RwLock<Wallet>>;
        let blockchain_lock: Arc<RwLock<Blockchain>>;
        let publickey: SaitoPublicKey;
        let privatekey: SaitoPrivateKey;

        {
            let mut t = TestManager::new();
            t.initialize(100, 720_000).await;
            t.wait_for_mining_event().await;

            wallet_lock = t.get_wallet_lock();
            mempool_lock = t.get_mempool_lock();
            blockchain_lock = t.get_blockchain_lock();
        }

        {
            let mut wallet = wallet_lock.write().await;
            publickey = wallet.get_publickey();
            privatekey = wallet.get_privatekey();
        }

        let ts = create_timestamp();
        let next_block_timestamp = ts + (HEARTBEAT * 2);

        let mut mempool = mempool_lock.write().await;
        let txs = Vec::<Transaction>::new();

        assert_eq!(mempool.get_routing_work_available(), 0);

        for _i in 0..5 {
            let mut tx = Transaction::new();

            {
                let mut wallet = wallet_lock.write().await;
                let (inputs, outputs) = wallet.generate_slips(720_000);
                tx.inputs = inputs;
                tx.outputs = outputs;
                // _i prevents sig from being identical during test
                // and thus from being auto-rejected from mempool
                tx.set_timestamp(ts + 120000 + _i);
                tx.generate(publickey);
                tx.sign(privatekey);
            }

            tx.add_hop(wallet_lock.clone(), publickey).await;

            mempool.add_transaction(tx).await;
        }

        assert_eq!(mempool.transactions.len(), 5);
        assert_eq!(mempool.get_routing_work_available(), 3_600_000);

        // TODO : FIX THIS TEST
        // assert_eq!(
        //     mempool.can_bundle_block(blockchain_lock.clone(), ts).await,
        //     false
        // );
        assert_eq!(
            mempool
                .can_bundle_block(blockchain_lock.clone(), ts + 120000)
                .await,
            true
        );
    }
}

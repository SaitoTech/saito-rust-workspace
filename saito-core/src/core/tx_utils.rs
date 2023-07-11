use crate::common::defs::{
    push_lock, SaitoPublicKey, StatVariable, Timestamp, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
    LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET, STAT_BIN_COUNT,
};
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;
use crate::{lock_for_read, lock_for_write};
use log::info;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn gen_tx(
    _wallet_lock: Arc<RwLock<Wallet>>,
    latest_block_id: u64,
    pubkey: SaitoPublicKey,
) -> Vec<Transaction> {
    info!("generating mock transactions");
    let mut transactions = Vec::new();
    let txs_to_generate = 10;
    let bytes_per_tx = 1024;
    let public_key;
    let private_key;

    {
        let (wallet, _wallet_) = lock_for_read!(_wallet_lock, LOCK_ORDER_WALLET);
        public_key = wallet.public_key;
        private_key = wallet.private_key;
    }

    {
        if latest_block_id == 0 {
            let mut vip_transaction = Transaction::create_vip_transaction(public_key, 50_000_000);
            vip_transaction.sign(&private_key);

            transactions.push(vip_transaction);

            let mut vip_transaction = Transaction::create_vip_transaction(pubkey, 50_000_000);
            vip_transaction.sign(&private_key);

            transactions.push(vip_transaction);
        }
    }

    let (mut wallet, _wallet_) = lock_for_write!(_wallet_lock, LOCK_ORDER_WALLET);

    for _i in 0..txs_to_generate {
        let mut transaction;
        transaction = Transaction::create(&mut wallet, public_key, 5000, 5000, false).unwrap();
        // TODO : generate a message buffer which can be converted back into JSON
        transaction.data = (0..bytes_per_tx).map(|_| rand::random::<u8>()).collect();
        transaction.generate(&public_key, 0, 0);
        transaction.sign(&private_key);

        transaction.add_hop(&private_key, &public_key, &public_key);
        transactions.push(transaction);
    }
    info!("generated transaction count: {:?}", txs_to_generate);
    transactions
}

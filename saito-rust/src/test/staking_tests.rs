#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use log::info;
    use tokio::sync::RwLock;

    use saito_core::core::data::block::Block;
    use saito_core::core::data::blockchain::Blockchain;
    use saito_core::core::data::slip::{Slip, SlipType};
    use saito_core::core::data::staking::Staking;
    use saito_core::core::data::transaction::Transaction;
    use saito_core::core::data::wallet::Wallet;

    use crate::test::test_manager::{create_timestamp, TestManager};

    //
    // does adding staking slips in different orders give us the same
    // results?
    //
    #[test]
    fn staking_add_staker_slips_in_different_order_and_check_sorting_works() {
        let mut staking1 = Staking::new();
        let mut staking2 = Staking::new();

        let mut slip1 = Slip::new();
        slip1.set_amount(1);
        slip1.set_slip_type(SlipType::StakerDeposit);

        let mut slip2 = Slip::new();
        slip2.set_amount(2);
        slip2.set_slip_type(SlipType::StakerDeposit);

        let mut slip3 = Slip::new();
        slip3.set_amount(3);
        slip3.set_slip_type(SlipType::StakerDeposit);

        let mut slip4 = Slip::new();
        slip4.set_amount(4);
        slip4.set_slip_type(SlipType::StakerDeposit);

        let mut slip5 = Slip::new();
        slip5.set_amount(5);
        slip5.set_slip_type(SlipType::StakerDeposit);

        staking1.add_staker(slip1.clone());
        assert_eq!(staking1.stakers.len(), 1);
        staking1.add_staker(slip2.clone());
        assert_eq!(staking1.stakers.len(), 2);
        staking1.add_staker(slip3.clone());
        assert_eq!(staking1.stakers.len(), 3);
        staking1.add_staker(slip4.clone());
        assert_eq!(staking1.stakers.len(), 4);
        staking1.add_staker(slip5.clone());
        assert_eq!(staking1.stakers.len(), 5);

        staking2.add_staker(slip2.clone());
        staking2.add_staker(slip4.clone());
        staking2.add_staker(slip1.clone());
        staking2.add_staker(slip5.clone());
        staking2.add_staker(slip3.clone());
        staking2.add_staker(slip2.clone());
        staking2.add_staker(slip1.clone());

        assert_eq!(staking1.stakers.len(), 5);
        assert_eq!(staking2.stakers.len(), 5);

        for i in 0..staking2.stakers.len() {
            info!(
                "{} -- {}",
                staking1.stakers[i].get_amount(),
                staking2.stakers[i].get_amount()
            );
        }

        for i in 0..staking2.stakers.len() {
            assert_eq!(
                staking1.stakers[i].clone().serialize_for_net(),
                staking2.stakers[i].clone().serialize_for_net()
            );
            assert_eq!(
                staking1.stakers[i]
                    .clone()
                    .compare(staking2.stakers[i].clone()),
                3
            ); // 3 = the same
        }
    }

    //
    // do staking deposits work properly and create proper payouts?
    //
    #[test]
    fn staking_add_deposit_slip_and_payout_calculation_test() {
        let mut staking = Staking::new();

        let mut slip1 = Slip::new();
        slip1.set_amount(200_000_000);
        slip1.set_slip_type(SlipType::StakerDeposit);

        let mut slip2 = Slip::new();
        slip2.set_amount(300_000_000);
        slip2.set_slip_type(SlipType::StakerDeposit);

        let mut slip3 = Slip::new();
        slip3.set_amount(400_000_000);
        slip3.set_slip_type(SlipType::StakerDeposit);

        let mut slip4 = Slip::new();
        slip4.set_amount(500_000_000);
        slip4.set_slip_type(SlipType::StakerDeposit);

        let mut slip5 = Slip::new();
        slip5.set_amount(600_000_000);
        slip5.set_slip_type(SlipType::StakerDeposit);

        staking.add_deposit(slip1);
        staking.add_deposit(slip2);
        staking.add_deposit(slip3);
        staking.add_deposit(slip4);
        staking.add_deposit(slip5);

        let (_res_spend, _res_unspend, _res_delete) = staking.reset_staker_table(1_000_000_000); // 10 Saito

        assert_eq!(
            staking.stakers[4].get_amount() + staking.stakers[4].get_payout(),
            210000000
        );
        assert_eq!(
            staking.stakers[3].get_amount() + staking.stakers[3].get_payout(),
            315000000
        );
        assert_eq!(
            staking.stakers[2].get_amount() + staking.stakers[2].get_payout(),
            420000000
        );
        assert_eq!(
            staking.stakers[1].get_amount() + staking.stakers[1].get_payout(),
            525000000
        );
        assert_eq!(
            staking.stakers[0].get_amount() + staking.stakers[0].get_payout(),
            630000000
        );
    }

    //
    // do we get proper results removing stakers and adding to pending? this is
    // important because we rely on remove_stakers() to not remove non-existing
    // entities, otherwise we need more complicated dupe-detection code.
    //
    #[test]
    fn staking_remove_staker_code_handles_duplicates_properly() {
        let mut staking = Staking::new();

        let mut slip1 = Slip::new();
        slip1.set_amount(200_000_000);
        slip1.set_slip_type(SlipType::StakerDeposit);

        let mut slip2 = Slip::new();
        slip2.set_amount(300_000_000);
        slip2.set_slip_type(SlipType::StakerDeposit);

        let mut slip3 = Slip::new();
        slip3.set_amount(400_000_000);
        slip3.set_slip_type(SlipType::StakerDeposit);

        let mut slip4 = Slip::new();
        slip4.set_amount(500_000_000);
        slip4.set_slip_type(SlipType::StakerDeposit);

        let mut slip5 = Slip::new();
        slip5.set_amount(600_000_000);
        slip5.set_slip_type(SlipType::StakerDeposit);

        staking.add_deposit(slip1.clone());
        staking.add_deposit(slip2.clone());
        staking.add_deposit(slip3.clone());
        staking.add_deposit(slip4.clone());
        staking.add_deposit(slip5.clone());

        let (_res_spend, _res_unspend, _res_delete) = staking.reset_staker_table(1_000_000_000); // 10 Saito

        assert_eq!(staking.stakers.len(), 5);
        assert_eq!(staking.remove_staker(slip1.clone()), true);
        assert_eq!(staking.remove_staker(slip2.clone()), true);
        assert_eq!(staking.remove_staker(slip1.clone()), false);
        assert_eq!(staking.stakers.len(), 3);
        assert_eq!(staking.remove_staker(slip5.clone()), true);
        assert_eq!(staking.remove_staker(slip5.clone()), false);
        assert_eq!(staking.stakers.len(), 2);
    }

    //
    // will staking payouts and the reset / rollover of the staking table work
    // properly with single-payouts per block?
    //
    #[tokio::test]
    #[serial_test::serial]
    async fn staking_create_blockchain_with_two_staking_deposits_one_staker_payout_per_block() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        //
        // initialize blockchain staking table
        //
        {
            let mut blockchain = blockchain_lock.write().await;
            let wallet = wallet_lock.read().await;
            let publickey = wallet.get_publickey();

            let mut slip1 = Slip::new();
            slip1.set_amount(200_000_000);
            slip1.set_slip_type(SlipType::StakerDeposit);

            let mut slip2 = Slip::new();
            slip2.set_amount(300_000_000);
            slip2.set_slip_type(SlipType::StakerDeposit);

            slip1.set_publickey(publickey);
            slip2.set_publickey(publickey);

            slip1.generate_utxoset_key();
            slip2.generate_utxoset_key();

            // add to utxoset
            slip1.on_chain_reorganization(&mut blockchain.utxoset, true, 1);
            slip2.on_chain_reorganization(&mut blockchain.utxoset, true, 1);

            blockchain.staking.add_deposit(slip1);
            blockchain.staking.add_deposit(slip2);

            blockchain.staking.reset_staker_table(1_000_000_000); // 10 Saito
        }

        let current_timestamp = create_timestamp();

        //
        // BLOCK 1
        //
        let block1 = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 3, 0, false, vec![])
            .await;
        let block1_hash = block1.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block1,
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        //
        // BLOCK 2
        //
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
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        //
        // we have yet to find a single golden ticket, so all in stakers
        //
        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 2);
            assert_eq!(blockchain.staking.pending.len(), 0);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 3
        //
        let block3 = test_manager
            .generate_block_and_metadata(
                block2_hash,
                current_timestamp + 240000,
                0,
                1,
                true,
                vec![],
            )
            .await;
        let block3_hash = block3.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block3,
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        //
        // we have found a single golden ticket, so we have paid a single staker
        //
        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 1);
            assert_eq!(blockchain.staking.pending.len(), 1);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 4
        //
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
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 1);
            assert_eq!(blockchain.staking.pending.len(), 1);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 5
        //
        test_manager.set_latest_block_hash(block4_hash);
        test_manager
            .add_block(current_timestamp + 480000, 0, 1, true, vec![])
            .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 2);
            assert_eq!(blockchain.staking.pending.len(), 0);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 6
        //
        test_manager
            .add_block(current_timestamp + 600000, 0, 1, false, vec![])
            .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 2);
            assert_eq!(blockchain.staking.pending.len(), 0);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 7
        //
        test_manager
            .add_block(current_timestamp + 720000, 0, 1, true, vec![])
            .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 1);
            assert_eq!(blockchain.staking.pending.len(), 1);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 8
        //
        test_manager
            .add_block(current_timestamp + 840000, 0, 1, false, vec![])
            .await;

        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 1);
            assert_eq!(blockchain.staking.pending.len(), 1);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 9
        //
        test_manager
            .add_block(current_timestamp + 960000, 0, 1, true, vec![])
            .await;
        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 2);
            assert_eq!(blockchain.staking.pending.len(), 0);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }
    }

    //
    // will staking payouts and the reset / rollover of the staking table work
    // properly with recurvise staking payouts that stretch back at least two blocks
    //
    #[tokio::test]
    #[serial_test::serial]
    async fn staking_create_blockchain_with_many_staking_deposits_many_staker_payouts_per_block() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        //
        // initialize blockchain staking table
        //
        {
            let mut blockchain = blockchain_lock.write().await;
            let wallet = wallet_lock.read().await;
            let publickey = wallet.get_publickey();

            let mut slip1 = Slip::new();
            slip1.set_amount(200_000_000);
            slip1.set_slip_type(SlipType::StakerDeposit);

            let mut slip2 = Slip::new();
            slip2.set_amount(300_000_000);
            slip2.set_slip_type(SlipType::StakerDeposit);

            let mut slip3 = Slip::new();
            slip3.set_amount(400_000_000);
            slip3.set_slip_type(SlipType::StakerDeposit);

            slip1.set_publickey(publickey);
            slip2.set_publickey(publickey);
            slip3.set_publickey(publickey);

            slip1.generate_utxoset_key();
            slip2.generate_utxoset_key();
            slip3.generate_utxoset_key();

            // add to utxoset
            slip1.on_chain_reorganization(&mut blockchain.utxoset, true, 1);
            slip2.on_chain_reorganization(&mut blockchain.utxoset, true, 1);
            slip3.on_chain_reorganization(&mut blockchain.utxoset, true, 1);

            blockchain.staking.add_deposit(slip1);
            blockchain.staking.add_deposit(slip2);
            blockchain.staking.add_deposit(slip3);

            blockchain.staking.reset_staker_table(1_000_000_000); // 10 Saito
        }

        let current_timestamp = create_timestamp();

        //
        // BLOCK 1
        //
        let block1 = test_manager
            .generate_block_and_metadata([0; 32], current_timestamp, 10, 0, false, vec![])
            .await;
        let block1_hash = block1.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block1,
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        //
        // BLOCK 2
        //
        test_manager.set_latest_block_hash(block1_hash);
        test_manager
            .add_block(current_timestamp + 120000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 3
        //
        test_manager
            .add_block(current_timestamp + 240000, 0, 1, false, vec![])
            .await;

        //
        // we have yet to find a single golden ticket, so all in stakers
        //
        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.staking.stakers.len(), 3);
            assert_eq!(blockchain.staking.pending.len(), 0);
            assert_eq!(blockchain.staking.deposits.len(), 0);
        }

        //
        // BLOCK 4 - GOLDEN TICKET
        //
        test_manager
            .add_block(current_timestamp + 360000, 0, 1, true, vec![])
            .await;

        //
        // BLOCK 5
        //
        test_manager
            .add_block(current_timestamp + 480000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 6
        //
        test_manager
            .add_block(current_timestamp + 600000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 7
        //
        test_manager
            .add_block(current_timestamp + 720000, 0, 1, true, vec![])
            .await;

        //
        // BLOCK 8
        //
        test_manager
            .add_block(current_timestamp + 840000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 9
        //
        test_manager
            .add_block(current_timestamp + 960000, 0, 1, true, vec![])
            .await;

        // staking must have been handled properly for all blocks to validate
        {
            let blockchain = blockchain_lock.read().await;
            assert_eq!(blockchain.get_latest_block_id(), 9);
            assert_eq!(
                3,
                blockchain.staking.stakers.len()
                    + blockchain.staking.pending.len()
                    + blockchain.staking.deposits.len()
            );
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn blockchain_staking_deposits_test() {
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
        let publickey;
        {
            let wallet = wallet_lock.read().await;
            publickey = wallet.get_publickey();
        }

        //
        // BLOCK 1 -- VIP transactions
        //
        test_manager
            .add_block(current_timestamp, 10, 0, false, vec![])
            .await;
        let block1_hash;

        {
            let blockchain = blockchain_lock.read().await;
            block1_hash = blockchain.get_latest_block_hash();
        }

        //
        // BLOCK 2 -- staking deposits
        //
        let mut stx1: Transaction;
        let mut stx2: Transaction;
        {
            let mut wallet = wallet_lock.write().await;
            stx1 = wallet.create_staking_deposit_transaction(100000).await;
            stx2 = wallet.create_staking_deposit_transaction(200000).await;
            stx1.generate_metadata(publickey);
            stx2.generate_metadata(publickey);
        }
        let mut transactions: Vec<Transaction> = vec![];
        transactions.push(stx1);
        transactions.push(stx2);
        let mut block2 = Block::generate(
            &mut transactions,
            block1_hash,
            wallet_lock.clone(),
            blockchain_lock.clone(),
            current_timestamp + 120000,
        )
        .await;
        block2.generate_metadata();
        let block2_hash = block2.get_hash();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block2,
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        info!("- AFTER BLOCK 2 - deposit");

        //
        // BLOCK 3 - payout
        //
        test_manager.set_latest_block_hash(block2_hash);
        test_manager
            .add_block(current_timestamp + 240000, 0, 1, true, vec![])
            .await;

        info!("- AFTER BLOCK 3 - PAYOUT has happened -");

        //
        // we have yet to find a single golden ticket, so all in stakers
        //
        {
            let blockchain = blockchain_lock.write().await;
            info!("STAKERS {:?}", blockchain.staking.stakers);
            info!("PENDING {:?}", blockchain.staking.pending);
            info!("DEPOSITS {:?}", blockchain.staking.deposits);
        }

        //
        // BLOCK 4
        //
        test_manager
            .add_block(current_timestamp + 360000, 0, 1, false, vec![])
            .await;

        //
        // we have yet to find a single golden ticket, so all in stakers
        //
        {
            let blockchain = blockchain_lock.write().await;
            info!("AFTER BLOCK #4");
            info!("STAKERS {:?}", blockchain.staking.stakers);
            info!("PENDING {:?}", blockchain.staking.pending);
            info!("DEPOSITS {:?}", blockchain.staking.deposits);
        }

        //
        // BLOCK 5
        //
        test_manager
            .add_block(current_timestamp + 480000, 0, 1, true, vec![])
            .await;
        let block5_hash;
        {
            let blockchain = blockchain_lock.read().await;
            block5_hash = blockchain.get_latest_block_hash();
        }

        //
        // BLOCK 6 -- withdraw a staking deposit
        //
        let mut wstx1: Transaction;
        {
            let mut wallet = wallet_lock.write().await;
            let blockchain = blockchain_lock.write().await;
            wstx1 = wallet
                .create_staking_withdrawal_transaction(&blockchain.staking)
                .await;
            wstx1.generate_metadata(publickey);
        }
        let mut transactions: Vec<Transaction> = vec![];
        info!("----------");
        info!("- WITHDRAW STAKER SLIP --");
        info!("----------");
        info!("---{:?}---", wstx1);
        info!("----------");
        transactions.push(wstx1);
        let mut block6 = Block::generate(
            &mut transactions,
            block5_hash,
            wallet_lock.clone(),
            blockchain_lock.clone(),
            current_timestamp + 600000,
        )
        .await;
        block6.generate_metadata();
        let block6_id = block6.get_id();
        TestManager::add_block_to_blockchain(
            blockchain_lock.clone(),
            block6,
            &mut test_manager.io_handler,
            test_manager.peers.clone(),
            test_manager.sender_to_miner.clone(),
        )
        .await;

        {
            let blockchain = blockchain_lock.read().await;
            blockchain.print();
            info!(
                "LATESTID: {} / {}",
                block6_id,
                blockchain.get_latest_block_id()
            );
            assert_eq!(blockchain.get_latest_block_id(), 6);
            assert_eq!(
                blockchain.staking.stakers.len() + blockchain.staking.pending.len(),
                1
            );
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn blockchain_roll_forward_staking_table_test_with_test_manager() {
        TestManager::clear_data_folder().await;
        let wallet_lock = Arc::new(RwLock::new(Wallet::new()));
        let blockchain_lock = Arc::new(RwLock::new(Blockchain::new(wallet_lock.clone())));
        let (sender_miner, receiver_miner) = tokio::sync::mpsc::channel(10);
        let mut test_manager = TestManager::new(
            blockchain_lock.clone(),
            wallet_lock.clone(),
            sender_miner.clone(),
        );

        let publickey;
        let current_timestamp = create_timestamp();

        {
            let wallet = wallet_lock.read().await;
            publickey = wallet.get_publickey();
            info!("publickey: {:?}", publickey);
        }

        //
        // BLOCK 1
        //
        test_manager
            .add_block(current_timestamp, 3, 0, false, vec![])
            .await;

        //
        // BLOCK 2
        //
        test_manager
            .add_block(current_timestamp + 120000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 3
        //
        test_manager
            .add_block(current_timestamp + 240000, 0, 1, false, vec![])
            .await;

        //
        // BLOCK 4
        //
        test_manager
            .add_block(current_timestamp + 360000, 0, 1, true, vec![])
            .await;

        test_manager.check_utxoset().await;
        test_manager.check_token_supply().await;
    }
}

// pub async fn check_token_supply(&self) {
//         println!("check_token_supply");
//         let mut token_supply: Currency = 0;
//         let mut current_supply: Currency = 0;
//         let mut block_inputs_amount: Currency;
//         let mut block_outputs_amount: Currency;
//         let mut previous_block_treasury: Currency;
//         let mut current_block_treasury: Currency = 0;
//         let mut unpaid_but_uncollected: Currency = 0;
//         let mut block_contains_fee_tx: bool;
//         let mut block_fee_tx_index: usize = 0;

//         let (blockchain, _blockchain_) =
//             lock_for_read!(self.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

//         let latest_block_id = blockchain.get_latest_block_id();

//         for i in 1..=latest_block_id {
//             let block_hash = blockchain
//                 .blockring
//                 .get_longest_chain_block_hash_by_block_id(i as u64);
//             let block = blockchain.get_block(&block_hash).unwrap();

//             block_inputs_amount = 0;
//             block_outputs_amount = 0;
//             block_contains_fee_tx = false;

//             previous_block_treasury = current_block_treasury;
//             current_block_treasury = block.treasury;

//             for t in 0..block.transactions.len() {
//                 //
//                 // we ignore the inputs in staking / fee transactions as they have
//                 // been pulled from the staking treasury and are already technically
//                 // counted in the money supply as an output from a previous slip.
//                 // we only care about the difference in token supply represented by
//                 // the difference in the staking_treasury.
//                 //
//                 if block.transactions[t].transaction_type == TransactionType::Fee {
//                     block_contains_fee_tx = true;
//                     block_fee_tx_index = t as usize;
//                 } else {
//                     for z in 0..block.transactions[t].from.len() {
//                         block_inputs_amount += block.transactions[t].from[z].amount;
//                     }
//                     for z in 0..block.transactions[t].to.len() {
//                         block_outputs_amount += block.transactions[t].to[z].amount;
//                     }
//                     println!("block_inputs_amount {}", block_inputs_amount);
//                     println!("block_outputs_amount {}", block_outputs_amount);
//                 }

//                 //
//                 // block one sets circulation
//                 //
//                 if i == 1 {
//                     token_supply = block_outputs_amount + block.treasury + block.staking_treasury;
//                     current_supply = token_supply;
//                     println!("token_supply {}", token_supply);
//                     println!("current_supply {}", current_supply);
//                 } else {
//                     //
//                     // figure out how much is in circulation
//                     //
//                     if block_contains_fee_tx == false {
//                         current_supply -= block_inputs_amount;
//                         current_supply += block_outputs_amount;

//                         unpaid_but_uncollected += block_inputs_amount;
//                         unpaid_but_uncollected -= block_outputs_amount;

//                         //
//                         // treasury increases must come here uncollected
//                         //
//                         if current_block_treasury > previous_block_treasury {
//                             unpaid_but_uncollected -=
//                                 current_block_treasury - previous_block_treasury;
//                         }
//                     } else {
//                         //
//                         // calculate total amount paid
//                         //
//                         let mut total_fees_paid: Currency = 0;
//                         let fee_transaction = &block.transactions[block_fee_tx_index];
//                         for output in fee_transaction.to.iter() {
//                             total_fees_paid += output.amount;
//                         }

//                         current_supply -= block_inputs_amount;
//                         current_supply += block_outputs_amount;
//                         current_supply += total_fees_paid;

//                         unpaid_but_uncollected += block_inputs_amount;
//                         unpaid_but_uncollected -= block_outputs_amount;
//                         unpaid_but_uncollected -= total_fees_paid;

//                         //
//                         // treasury increases must come here uncollected
//                         //
//                         if current_block_treasury > previous_block_treasury {
//                             unpaid_but_uncollected -=
//                                 current_block_treasury - previous_block_treasury;
//                         }
//                     }

//                     //
//                     // token supply should be constant
//                     //
//                     let total_in_circulation = current_supply
//                         + unpaid_but_uncollected
//                         + block.treasury
//                         + block.staking_treasury;

//                     //
//                     // we check that overall token supply has not changed
//                     //
//                     assert_eq!(total_in_circulation, token_supply);
//                 }
//             }
//         }
//     }

//
// TODO
// - array is not the best data structure
// - insert needs to place into a specified position, probabaly ordered by publickey and then UUID
//

use bigint::uint::U256;
use log::{info, trace};

use crate::common::defs::SaitoHash;
use crate::core::data::block::Block;
use crate::core::data::blockchain::GENESIS_PERIOD;
use crate::core::data::crypto::hash;
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::slip::{Slip, SlipType};
use crate::core::data::transaction::TransactionType;

#[derive(Debug, Clone)]
pub struct Staking {
    // deposits waiting to join staking table for the first time
    pub deposits: Vec<Slip>,
    // in staking table waiting for selection / payout
    pub stakers: Vec<Slip>,
    // waiting for reset of staking table
    pub pending: Vec<Slip>,
}

impl Staking {
    pub fn new() -> Staking {
        Staking {
            deposits: vec![],
            stakers: vec![],
            pending: vec![],
        }
    }

    pub fn find_winning_staker(&self, random_number: SaitoHash) -> Option<Slip> {
        if self.stakers.is_empty() {
            return None;
        }

        //
        // find winning staker
        //
        let x = U256::from_big_endian(&random_number);
        let y = self.stakers.len();
        let z = U256::from_big_endian(&y.to_be_bytes());
        let (zy, _bolres) = x.overflowing_rem(z);

        let retrieve_from_pos = zy.low_u64();

        let winning_slip = self.stakers[retrieve_from_pos as usize].clone();

        Some(winning_slip)
    }

    //
    // resets the staker table
    //
    // without using floating-point division, we calculate the share that each staker
    // should earn of the upcoming sweep through the stakers table, and insert the
    // pending and pending-deposits slips into the staking table with the updated
    // expected payout.
    //
    // returns three vectors with slips to SPEND, UNSPEND, DELETE
    //
    // wny?
    //
    // we cannot pass the UTXOSet into the staking object to update as that would
    // require multiple mutable borrows of the blockchain object, so we receive
    // vectors of the slips that need to be inserted, spent or deleted in the
    // blockchain and handle after-the-fact. This permits keeping the UTXOSET
    // up-to-date with the staking tables.
    //
    pub fn reset_staker_table(
        &mut self,
        staking_treasury: u64,
    ) -> (Vec<Slip>, Vec<Slip>, Vec<Slip>) {
        info!("===========================");
        info!("=== RESET STAKING TABLE ===");
        info!("===========================");

        let res_spend: Vec<Slip> = vec![];
        let res_unspend: Vec<Slip> = vec![];
        let res_delete: Vec<Slip> = vec![];

        //
        // move pending into staking table
        //
        for i in 0..self.pending.len() {
            self.add_staker(self.pending[i].clone());
        }
        for i in 0..self.deposits.len() {
            self.add_staker(self.deposits[i].clone());
        }
        self.pending = vec![];
        self.deposits = vec![];

        if self.stakers.is_empty() {
            return (res_spend, res_unspend, res_delete);
        }

        //
        // adjust the slip amounts based on genesis period
        //
        let staking_payout_per_block: u64 = staking_treasury / GENESIS_PERIOD;

        //
        // calculate average amount staked
        //
        let mut total_staked: u64 = 0;
        for i in 0..self.stakers.len() {
            total_staked += self.stakers[i].get_amount();
        }
        let average_staked = total_staked / self.stakers.len() as u64;

        //
        // calculate the payout for average stake
        //
        let m = U256::from_big_endian(&staking_payout_per_block.to_be_bytes());
        let p = U256::from_big_endian(&self.stakers.len().to_be_bytes());

        let (q, _r) = m.overflowing_div(p);
        let average_staker_payout = q.as_u64();

        //
        // and adjust the payout based on this....
        //
        for i in 0..self.stakers.len() {
            //
            // get the total staked
            //
            let my_staked_amount = self.stakers[i].get_amount();

            //
            // figure how much we are due...
            //
            // my stake PLUS (my stake / 1 * ( my_stake / average_staked )
            // my stake PLUS (my stake / 1 * ( my_stake / average_staked ) * ( ( treasury / genesis_period )
            // my stake PLUS (my stake / 1 * ( my_stake / average_staked ) * ( ( treasury / genesis_period )
            //
            let a = U256::from_big_endian(&my_staked_amount.to_be_bytes());
            let b = U256::from_big_endian(&average_staker_payout.to_be_bytes());
            let nominator: U256 = a.saturating_mul(b);
            let denominator = U256::from_big_endian(&average_staked.to_be_bytes());

            let (z, f) = nominator.overflowing_div(denominator);

            let mut staking_profit: u64 = 0;
            if !f {
                staking_profit = z.as_u64();
            }

            self.stakers[i].set_payout(staking_profit);
        }

        (res_spend, res_unspend, res_delete)
    }

    pub fn validate_slip_in_deposits(&self, slip: Slip) -> bool {
        for i in 0..self.deposits.len() {
            if slip.get_utxoset_key() == self.deposits[i].get_utxoset_key() {
                return true;
            }
        }
        false
    }

    pub fn validate_slip_in_stakers(&self, slip: Slip) -> bool {
        for i in 0..self.stakers.len() {
            if slip.get_utxoset_key() == self.stakers[i].get_utxoset_key() {
                return true;
            }
        }
        false
    }

    pub fn validate_slip_in_pending(&self, slip: Slip) -> bool {
        for i in 0..self.pending.len() {
            if slip.get_utxoset_key() == self.pending[i].get_utxoset_key() {
                return true;
            }
        }
        false
    }

    pub fn add_deposit(&mut self, slip: Slip) {
        self.deposits.push(slip);
    }

    //
    // slips are added in ascending order based on publickey and then
    // UUID. this is simply to ensure that chain reorgs do not cause
    // disagreements about which staker is selected.
    //
    pub fn add_staker(&mut self, slip: Slip) -> bool {
        //
        // TODO skip-hop algorithm instead of brute force
        //
        if self.stakers.len() == 0 {
            self.stakers.push(slip);
            return true;
        } else {
            for i in 0..self.stakers.len() {
                let how_compares = slip.compare(self.stakers[i].clone());
                // 1 - self is bigger
                // 2 - self is smaller
                // insert at position i
                if how_compares == 2 {
                    if self.stakers.len() == (i + 1) {
                        self.stakers.push(slip);
                        return true;
                    }
                } else {
                    if how_compares == 1 {
                        self.stakers.insert(i, slip);
                        return true;
                    }
                    if how_compares == 3 {
                        return false;
                    }
                }
            }

            self.stakers.push(slip);
            return true;
        }
    }

    pub fn add_pending(&mut self, slip: Slip) {
        self.pending.push(slip);
    }

    pub fn remove_deposit(&mut self, slip: Slip) -> bool {
        for i in 0..self.deposits.len() {
            if slip.get_utxoset_key() == self.deposits[i].get_utxoset_key() {
                let _removed_slip = self.deposits.remove(i);
                return true;
            }
        }
        false
    }

    pub fn remove_staker(&mut self, slip: Slip) -> bool {
        for i in 0..self.stakers.len() {
            if slip.get_utxoset_key() == self.stakers[i].get_utxoset_key() {
                let _removed_slip = self.stakers.remove(i);
                return true;
            }
        }
        false
    }

    pub fn remove_pending(&mut self, slip: Slip) -> bool {
        for i in 0..self.pending.len() {
            if slip.get_utxoset_key() == self.pending[i].get_utxoset_key() {
                let _removed_slip = self.pending.remove(i);
                return true;
            }
        }
        false
    }

    //
    // handle staking / pending / deposit tables
    //
    // returns slips to SPEND, UNSPEND and DELETE
    //
    // this is required as staking table controlled by blockchain and Rust
    // restricts us from passing the UTXOSET into this part of the program.
    //
    pub fn on_chain_reorganization(
        &mut self,
        block: &Block,
        longest_chain: bool,
    ) -> (Vec<Slip>, Vec<Slip>, Vec<Slip>) {
        let res_spend: Vec<Slip> = vec![];
        let res_unspend: Vec<Slip> = vec![];
        let res_delete: Vec<Slip> = vec![];

        //
        // add/remove deposits
        //
        for tx in block.get_transactions() {
            if tx.get_transaction_type() == TransactionType::StakerWithdrawal {
                //
                // someone has successfully withdrawn so we need to remove this slip
                // from the necessary table if moving forward, or add it back if
                // moving backwards.
                //
                // roll forward
                //
                if longest_chain {
                    if tx.inputs[0].get_slip_type() == SlipType::StakerWithdrawalPending {
                        self.remove_pending(tx.inputs[0].clone());
                    }
                    if tx.inputs[0].get_slip_type() == SlipType::StakerWithdrawalStaking {
                        self.remove_staker(tx.inputs[0].clone());
                    }
                    //
                    // roll backward
                    //
                } else {
                    if tx.inputs[0].get_slip_type() == SlipType::StakerWithdrawalPending {
                        self.add_pending(tx.inputs[0].clone());
                    }
                    if tx.inputs[0].get_slip_type() == SlipType::StakerWithdrawalStaking {
                        self.add_staker(tx.inputs[0].clone());
                    }
                }
            }

            if tx.get_transaction_type() == TransactionType::StakerDeposit {
                for i in 0..tx.outputs.len() {
                    if tx.outputs[i].get_slip_type() == SlipType::StakerDeposit {
                        //
                        // roll forward
                        //
                        if longest_chain {
                            self.add_deposit(tx.outputs[i].clone());
                            //
                            // roll backward
                            //
                        } else {
                            self.remove_deposit(tx.outputs[i].clone());
                        }
                    }
                }
            }
        }

        //
        // reset tables if needed
        //
        if longest_chain {
            //
            // reset stakers if necessary
            //
            if self.stakers.is_empty() {
                let (_res_spend, _res_unspend, _res_delete) =
                    self.reset_staker_table(block.get_staking_treasury());
            }
        } else {
            //
            // reset pending if necessary
            //
            if self.pending.is_empty() {
                self.pending = vec![];
                self.deposits = vec![];
                for i in 0..self.stakers.len() {
                    if self.stakers[i].get_slip_type() == SlipType::StakerOutput {
                        self.pending.push(self.stakers[i].clone());
                    }
                    if self.stakers[i].get_slip_type() == SlipType::StakerDeposit {
                        self.deposits.push(self.stakers[i].clone());
                    }
                }
                self.stakers = vec![];
            }
        }

        //
        // update staking tables
        //
        if block.get_has_fee_transaction() && block.get_has_golden_ticket() {
            let fee_transaction =
                &block.get_transactions()[block.get_fee_transaction_idx() as usize];

            let golden_ticket_transaction =
                &block.get_transactions()[block.get_golden_ticket_idx() as usize];

            //
            // grab random input from golden ticket
            //
            let golden_ticket: GoldenTicket =
                GoldenTicket::deserialize(golden_ticket_transaction.get_message().to_vec());

            // pick router and burn one
            let mut next_random_number = hash(&golden_ticket.get_random().to_vec());
            next_random_number = hash(&next_random_number.to_vec());
            next_random_number = hash(&next_random_number.to_vec());

            let mut is_there_a_staker_output = false;

            for i in 0..fee_transaction.outputs.len() {
                if fee_transaction.outputs[i].get_slip_type() == SlipType::StakerOutput {
                    is_there_a_staker_output = true;
                }
            }
            if is_there_a_staker_output == false {
                return (res_spend, res_unspend, res_delete);
            }
            if fee_transaction.inputs.is_empty() {
                return (res_spend, res_unspend, res_delete);
            }

            //
            // roll forward
            //
            if longest_chain {
                //
                // re-create staker table, if needed
                //
                // we do this at both the start and the end of this function so that
                // we will always have a table that can be handled regardless of
                // vacillations in on_chain_reorg, such as resetting the table and
                // then non-longest-chaining the same block
                //
                trace!(
                    "Rolling forward and moving into pending: {}!",
                    self.stakers.len()
                );
                if self.stakers.is_empty() {
                    let (_res_spend, _res_unspend, _res_delete) =
                        self.reset_staker_table(block.get_staking_treasury());
                }

                //
                // process outbound staking payments
                //
                let mut slips_to_remove_from_staking = vec![];
                let mut slips_to_add_to_pending = vec![];

                let mut staker_slip_num = 0;
                for i in 0..fee_transaction.outputs.len() {
                    let staker_output = fee_transaction.outputs[i].clone();
                    // we have already handled all stakers
                    if fee_transaction.inputs.len() <= staker_slip_num {
                        break;
                    }
                    if staker_output.get_slip_type() == SlipType::StakerOutput {
                        // ROUTER BURNED FIRST
                        next_random_number = hash(&next_random_number.to_vec()); // router + burn
                        next_random_number = hash(&next_random_number.to_vec()); // burn second

                        //
                        // move staker to pending
                        //
                        let lucky_staker_option = self.find_winning_staker(next_random_number); // use first

                        if let Some(lucky_staker) = lucky_staker_option {
                            info!("the lucky staker is: {:?}", lucky_staker);
                            trace!(
                                "moving from staker into pending: {}",
                                lucky_staker.get_amount()
                            );

                            slips_to_remove_from_staking.push(lucky_staker.clone());
                            slips_to_add_to_pending.push(staker_output.clone());
                        }
                        staker_slip_num += 1;

                        // setup for router selection next loop
                        next_random_number = hash(&next_random_number.to_vec());
                    }
                }

                //
                // we handle the slips together like this as we can occasionally
                // get duplicates if the same slip is selected recursively, but
                // we do not pay out duplicates. so we only add to pending if we
                // successfully remove from the staker table.
                //
                for i in 0..slips_to_remove_from_staking.len() {
                    if self.remove_staker(slips_to_remove_from_staking[i].clone()) == true {
                        self.add_pending(slips_to_add_to_pending[i].clone());
                    }
                }

                //
                // re-create staker table, if needed
                //
                if self.stakers.is_empty() {
                    let (_res_spend, _res_unspend, _res_delete) =
                        self.reset_staker_table(block.get_staking_treasury());
                }

                //
                // roll backward
                //
            } else {
                info!("roll backward...");

                //
                // reset pending if necessary
                //
                if self.pending.is_empty() {
                    self.pending = vec![];
                    self.deposits = vec![];
                    for i in 0..self.stakers.len() {
                        if self.stakers[i].get_slip_type() == SlipType::StakerOutput {
                            self.pending.push(self.stakers[i].clone());
                        }
                        if self.stakers[i].get_slip_type() == SlipType::StakerDeposit {
                            self.deposits.push(self.stakers[i].clone());
                        }
                    }
                    self.stakers = vec![];
                }

                //
                // process outbound staking payments
                //
                let mut staker_slip_num = 0;
                for i in 0..fee_transaction.outputs.len() {
                    let staker_output = fee_transaction.outputs[i].clone();
                    if fee_transaction.inputs.len() < staker_slip_num {
                        break;
                    }
                    let staker_input = fee_transaction.inputs[staker_slip_num].clone();

                    if staker_output.get_slip_type() == SlipType::StakerOutput {
                        //
                        // remove from pending to staker (awaiting payout)
                        //
                        self.remove_pending(staker_output.clone());
                        let slip_type = staker_input.get_slip_type();
                        if slip_type == SlipType::StakerDeposit {
                            self.add_deposit(staker_input.clone());
                        }
                        if slip_type == SlipType::StakerOutput {
                            self.add_staker(staker_input.clone());
                        }

                        staker_slip_num += 1;
                    }
                }

                //
                // reset pending if necessary
                //
                if self.pending.is_empty() {
                    self.pending = vec![];
                    self.deposits = vec![];
                    for i in 0..self.stakers.len() {
                        if self.stakers[i].get_slip_type() == SlipType::StakerOutput {
                            self.pending.push(self.stakers[i].clone());
                        }
                        if self.stakers[i].get_slip_type() == SlipType::StakerDeposit {
                            self.deposits.push(self.stakers[i].clone());
                        }
                    }
                    self.stakers = vec![];
                }
            }
        }

        (res_spend, res_unspend, res_delete)
    }
}

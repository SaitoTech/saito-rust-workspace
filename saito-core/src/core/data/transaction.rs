use log::{debug, error, info, trace, warn};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use primitive_types::U256;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::common::defs::{
    Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, UtxoSet,
};
use crate::core::data::crypto::{hash, sign, verify, verify_signature};
use crate::core::data::hop::{Hop, HOP_SIZE};
use crate::core::data::slip::{Slip, SlipType, SLIP_SIZE};
use crate::core::data::wallet::Wallet;
use crate::iterate;

pub const TRANSACTION_SIZE: usize = 93;

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Clone, FromPrimitive)]
pub enum TransactionType {
    Normal = 0,
    /// Paying for the network
    Fee = 1,
    GoldenTicket = 2,
    ATR = 3,
    /// VIP transactions won't pay an ATR fee. (Issued to early investors)
    Vip = 4,
    SPV = 5,
    /// Issues funds for an address at the start of the network
    Issuance = 6,
    Other = 7,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Transaction {
    // the bulk of the consensus transaction data
    pub timestamp: u64,
    pub inputs: Vec<Slip>,
    pub outputs: Vec<Slip>,
    // #[serde(with = "serde_bytes")] TODO : check this for performance
    pub message: Vec<u8>,
    pub(crate) transaction_type: TransactionType,
    pub(crate) replaces_txs: u32,
    #[serde_as(as = "[_; 64]")]
    pub signature: SaitoSignature,
    path: Vec<Hop>,

    // hash used for merkle_root (does not include signature)
    pub hash_for_signature: Option<SaitoHash>,

    /// total nolan in input slips
    pub total_in: Currency,
    /// total nolan in output slips
    pub total_out: Currency,
    /// total fees
    pub total_fees: Currency,
    /// total work to creator
    pub total_work_for_me: Currency,
    /// cumulative fees for this tx-in-block
    pub cumulative_fees: Currency,
}

impl Default for Transaction {
    fn default() -> Self {
        Self {
            timestamp: 0,
            inputs: vec![],
            outputs: vec![],
            message: vec![],
            transaction_type: TransactionType::Normal,
            replaces_txs: 1,
            signature: [0; 64],
            hash_for_signature: None,
            path: vec![],
            total_in: 0,
            total_out: 0,
            total_fees: 0,
            total_work_for_me: 0,
            cumulative_fees: 0,
        }
    }
}

impl Transaction {
    pub fn add_hop(
        &mut self,
        my_private_key: &SaitoPrivateKey,
        my_public_key: &SaitoPublicKey,
        to_public_key: &SaitoPublicKey,
    ) {
        assert_ne!(my_public_key, to_public_key);
        let hop = Hop::generate(my_private_key, my_public_key, to_public_key, self);
        self.path.push(hop);
    }

    /// add input slip
    ///
    /// # Arguments
    ///
    /// * `input_slip`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn add_input(&mut self, input_slip: Slip) {
        self.inputs.push(input_slip);
    }

    /// add output slip
    ///
    /// # Arguments
    ///
    /// * `output_slip`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn add_output(&mut self, output_slip: Slip) {
        self.outputs.push(output_slip);
    }

    /// this function exists largely for testing. It attempts to attach the requested fee
    /// to the transaction if possible. If not possible it reverts back to a transaction
    /// with 1 zero-fee input and 1 zero-fee output.
    ///
    /// # Arguments
    ///
    /// * `wallet_lock`:
    /// * `to_publickey`:
    /// * `with_payment`:
    /// * `with_fee`:
    ///
    /// returns: Transaction
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn create(
        wallet: &mut Wallet,
        to_public_key: SaitoPublicKey,
        with_payment: Currency,
        with_fee: Currency,
    ) -> Transaction {
        trace!(
            "generating transaction : payment = {:?}, fee = {:?}",
            with_payment,
            with_fee
        );

        let available_balance = wallet.get_available_balance();
        let total_requested = with_payment + with_fee;
        trace!(
            "in generate transaction. available: {} and payment: {} and fee: {}",
            available_balance,
            with_payment,
            with_fee
        );

        if available_balance >= total_requested {
            let mut transaction = Transaction::default();
            let (mut input_slips, mut output_slips) = wallet.generate_slips(total_requested);
            let input_len = input_slips.len();
            let output_len = output_slips.len();

            for _i in 0..input_len {
                transaction.add_input(input_slips[0].clone());
                input_slips.remove(0);
            }
            for _i in 0..output_len {
                transaction.add_output(output_slips[0].clone());
                output_slips.remove(0);
            }

            // add the payment
            let mut output = Slip::default();
            output.public_key = to_public_key;
            output.amount = with_payment;
            transaction.add_output(output);

            transaction
        } else {
            if available_balance > with_payment {
                let mut transaction = Transaction::default();
                let (mut input_slips, mut output_slips) = wallet.generate_slips(total_requested);
                let input_len = input_slips.len();
                let output_len = output_slips.len();

                for _i in 0..input_len {
                    transaction.add_input(input_slips[0].clone());
                    input_slips.remove(0);
                }
                for _i in 0..output_len {
                    transaction.add_output(output_slips[0].clone());
                    output_slips.remove(0);
                }

                // add the payment
                let mut output = Slip::default();
                output.public_key = to_public_key;
                output.amount = with_payment;
                transaction.add_output(output);

                return transaction;
            }

            if available_balance > with_fee {
                let mut transaction = Transaction::default();
                let (mut input_slips, mut output_slips) = wallet.generate_slips(total_requested);
                let input_len = input_slips.len();
                let output_len = output_slips.len();

                for _i in 0..input_len {
                    transaction.add_input(input_slips[0].clone());
                    input_slips.remove(0);
                }
                for _i in 0..output_len {
                    transaction.add_output(output_slips[0].clone());
                    output_slips.remove(0);
                }

                return transaction;
            }

            //
            // we have neither enough for the payment OR the fee, so
            // we just create a transaction that has no payment AND no
            // attached fee.
            //
            let mut transaction = Transaction::default();

            let mut input1 = Slip::default();
            input1.public_key = to_public_key;
            input1.amount = 0;
            input1.block_id = 0;
            input1.tx_ordinal = 0;

            let mut output1 = Slip::default();
            output1.public_key = wallet.public_key;
            output1.block_id = 0;
            output1.tx_ordinal = 0;

            transaction.add_input(input1);
            transaction.add_output(output1);

            transaction
        }
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `to_publickey`:
    /// * `with_amount`:
    ///
    /// returns: Transaction
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```

    pub fn create_vip_transaction(
        to_public_key: SaitoPublicKey,
        with_amount: Currency,
    ) -> Transaction {
        debug!("generate vip transaction : amount = {:?}", with_amount);
        let mut transaction = Transaction::default();
        transaction.transaction_type = TransactionType::Vip;
        let mut output = Slip::default();
        output.public_key = to_public_key;
        output.amount = with_amount;
        output.slip_type = SlipType::VipOutput;
        transaction.add_output(output);
        transaction
    }

    /// create rebroadcast transaction
    ///
    /// # Arguments
    ///
    /// * `transaction_to_rebroadcast`:
    /// * `output_slip_to_rebroadcast`:
    /// * `with_fee`:
    /// * `with_staking_subsidy`:
    ///
    /// returns: Transaction
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn create_rebroadcast_transaction(
        transaction_to_rebroadcast: &Transaction,
        output_slip_to_rebroadcast: &Slip,
        with_fee: Currency,
        with_staking_subsidy: Currency,
    ) -> Transaction {
        let mut transaction = Transaction::default();
        let mut output_payment = 0;
        if output_slip_to_rebroadcast.amount > with_fee {
            output_payment = output_slip_to_rebroadcast.amount - with_fee + with_staking_subsidy;
        }

        transaction.transaction_type = TransactionType::ATR;

        let mut output = Slip::default();
        output.public_key = output_slip_to_rebroadcast.public_key;
        output.amount = output_payment;
        output.slip_type = SlipType::ATR;
        output.block_id = output_slip_to_rebroadcast.block_id;
        output.tx_ordinal = output_slip_to_rebroadcast.tx_ordinal;

        //
        // if this is the FIRST time we are rebroadcasting, we copy the
        // original transaction into the message field in serialized
        // form. this preserves the original message and its signature
        // in perpetuity.
        //
        // if this is the SECOND or subsequent rebroadcast, we do not
        // copy the ATR tx (no need for a meta-tx) and rather just update
        // the message field with the original transaction (which is
        // by definition already in the previous TX message space.
        //
        if output_slip_to_rebroadcast.slip_type == SlipType::ATR {
            transaction.message = transaction_to_rebroadcast.message.to_vec();
        } else {
            transaction.message = transaction_to_rebroadcast.serialize_for_net().to_vec();
        }

        transaction.add_output(output);

        //
        // signature is the ORIGINAL signature. this transaction
        // will fail its signature check and then get analysed as
        // a rebroadcast transaction because of its transaction type.
        //
        transaction.signature = transaction_to_rebroadcast.signature;

        transaction
    }

    //
    // removes utxoset entries when block is deleted
    //
    pub async fn delete(&self, utxoset: &mut UtxoSet) -> bool {
        self.inputs.iter().for_each(|input| {
            input.delete(utxoset);
        });
        self.outputs.iter().for_each(|output| {
            output.delete(utxoset);
        });

        true
    }

    /// Deserialize from bytes to a Transaction.
    /// [len of inputs - 4 bytes - u32]
    /// [len of outputs - 4 bytes - u32]
    /// [len of message - 4 bytes - u32]
    /// [len of path - 4 bytes - u32]
    /// [signature - 64 bytes - Secp25k1 sig]
    /// [timestamp - 8 bytes - u64]
    /// [transaction type - 1 byte]
    /// [input][input][input]...
    /// [output][output][output]...
    /// [message]
    /// [hop][hop][hop]...
    pub fn deserialize_from_net(bytes: &Vec<u8>) -> Transaction {
        let inputs_len: u32 = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let outputs_len: u32 = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let message_len: usize = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let path_len: usize = u32::from_be_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let signature: SaitoSignature = bytes[16..80].try_into().unwrap();
        let timestamp: u64 = u64::from_be_bytes(bytes[80..88].try_into().unwrap());
        let replaces_txs = u32::from_be_bytes(bytes[88..92].try_into().unwrap());
        let transaction_type: TransactionType = FromPrimitive::from_u8(bytes[92]).unwrap();
        let start_of_inputs = TRANSACTION_SIZE;
        let start_of_outputs = start_of_inputs + inputs_len as usize * SLIP_SIZE;
        let start_of_message = start_of_outputs + outputs_len as usize * SLIP_SIZE;
        let start_of_path = start_of_message + message_len;
        let mut inputs: Vec<Slip> = vec![];
        for n in 0..inputs_len {
            let start_of_data: usize = start_of_inputs as usize + n as usize * SLIP_SIZE;
            let end_of_data: usize = start_of_data + SLIP_SIZE;
            let input = Slip::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec());
            inputs.push(input);
        }
        let mut outputs: Vec<Slip> = vec![];
        for n in 0..outputs_len {
            let start_of_data: usize = start_of_outputs as usize + n as usize * SLIP_SIZE;
            let end_of_data: usize = start_of_data + SLIP_SIZE;
            let output = Slip::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec());
            outputs.push(output);
        }
        let message = bytes[start_of_message..start_of_message + message_len]
            .try_into()
            .unwrap();
        let mut path: Vec<Hop> = vec![];
        for n in 0..path_len {
            let start_of_data: usize = start_of_path as usize + n as usize * HOP_SIZE;
            let end_of_data: usize = start_of_data + HOP_SIZE;
            let hop = Hop::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec());
            path.push(hop);
        }

        let mut transaction = Transaction::default();
        transaction.timestamp = timestamp;
        transaction.inputs = inputs;
        transaction.outputs = outputs;
        transaction.message = message;
        transaction.replaces_txs = replaces_txs;
        transaction.transaction_type = transaction_type;
        transaction.signature = signature;
        transaction.path = path;
        transaction
    }

    pub fn is_fee_transaction(&self) -> bool {
        self.transaction_type == TransactionType::Fee
    }

    pub fn is_atr_transaction(&self) -> bool {
        self.transaction_type == TransactionType::ATR
    }

    pub fn is_golden_ticket(&self) -> bool {
        self.transaction_type == TransactionType::GoldenTicket
    }

    pub fn is_issuance_transaction(&self) -> bool {
        self.transaction_type == TransactionType::Issuance
    }

    //
    // generates all non-cumulative
    //
    pub fn generate(&mut self, public_key: &SaitoPublicKey, tx_index: u64, block_id: u64) -> bool {
        //
        // nolan_in, nolan_out, total fees
        //
        self.generate_total_fees(tx_index, block_id);

        //
        // routing work for asserted public_key
        //
        self.generate_total_work(public_key);

        //
        // ensure hash exists for signing
        //
        // self.generate_hash_for_signature();

        true
    }

    //
    // calculate cumulative fee share in block
    //
    pub fn generate_cumulative_fees(&mut self, cumulative_fees: Currency) -> Currency {
        self.cumulative_fees = cumulative_fees + self.total_fees;
        self.cumulative_fees
    }

    //
    // calculate total fees in block
    //
    pub fn generate_total_fees(&mut self, tx_index: u64, block_id: u64) {
        // TODO - remove for uuid work
        // generate tx signature hash
        self.generate_hash_for_signature();
        trace!(
            "generating total fees for tx : {:?}",
            hex::encode(self.hash_for_signature.unwrap())
        );

        // calculate nolan in / out, fees
        // generate utxoset key for every slip
        let nolan_in = self
            .inputs
            .iter_mut()
            .map(|slip| {
                slip.generate_utxoset_key();
                slip.amount
            })
            .sum::<Currency>();

        let nolan_out = self
            .outputs
            .iter_mut()
            .enumerate()
            .map(|(index, slip)| {
                if slip.slip_type != SlipType::ATR {
                    slip.block_id = block_id;
                    slip.tx_ordinal = tx_index;
                    slip.slip_index = index as u8;
                }
                slip.generate_utxoset_key();
                slip.amount
            })
            .sum::<Currency>();

        self.total_in = nolan_in;
        self.total_out = nolan_out;
        self.total_fees = 0;

        //
        // note that this is not validation code, permitting this. we may have
        // some transactions that do insert NOLAN, such as during testing of
        // monetary policy. All sanity checks need to be in the validate()
        // function.
        //
        if nolan_in > nolan_out {
            self.total_fees = nolan_in - nolan_out;
        }
    }
    //
    // calculate cumulative routing work in block
    //
    pub fn generate_total_work(&mut self, public_key: &SaitoPublicKey) {
        //
        // if there is no routing path, then the transaction contains
        // no usable work for producing a block, and any payout associated
        // with the transaction will simply be issued to the creator of
        // the transaction itself.
        //
        if self.path.is_empty() {
            self.total_work_for_me = 0;
            return;
        }

        //
        // something is wrong if we are not the last routing node
        //
        let last_hop = &self.path[self.path.len() - 1];
        if last_hop.to.ne(public_key) {
            self.total_work_for_me = 0;
            warn!(
                "tx : {:?} last hop is not current node",
                hex::encode(self.signature)
            );
            return;
        }

        let total_fees = self.total_fees;
        let mut routing_work_available_to_public_key = total_fees;

        //
        // first hop gets ALL the routing work, so we start
        // halving from the 2nd hop in the routing path
        //
        for i in 1..self.path.len() {
            // TODO : check if this check required here since txs already validated at this point
            if self.path[i].from != self.path[i - 1].to {
                self.total_work_for_me = 0;
                warn!(
                    "tx : {:?} from and to not matching. to : {:?} from : {:?}",
                    hex::encode(self.signature),
                    hex::encode(self.path[i - 1].to),
                    hex::encode(self.path[i].from)
                );
                return;
            }

            // otherwise halve the work
            let half_of_routing_work: Currency = routing_work_available_to_public_key / 2;
            routing_work_available_to_public_key -= half_of_routing_work;
        }

        self.total_work_for_me = routing_work_available_to_public_key;
        trace!(
            "total work : {:?} for tx : {:?}. total fees : {:?} total in : {:?} total_out : {:?}",
            self.total_work_for_me,
            hex::encode(self.signature),
            self.total_fees,
            self.total_in,
            self.total_out
        );
    }

    //
    // generate hash used for signing the tx
    //
    pub fn generate_hash_for_signature(&mut self) {
        self.hash_for_signature = Some(hash(&self.serialize_for_signature()));
    }

    pub fn get_winning_routing_node(&self, random_hash: SaitoHash) -> SaitoPublicKey {
        //
        // if there are no routing paths, we return the sender of
        // the payment, as they're got all of the routing work by
        // definition. this is the edge-case where sending a tx
        // can make you money.
        //
        if self.path.is_empty() {
            if !self.inputs.is_empty() {
                return self.inputs[0].public_key;
            } else {
                return [0; 33];
            }
        }

        //
        // no winning transaction should have no fees unless the
        // entire block has no fees, in which case we have a block
        // without any fee-paying transactions.
        //
        // burn these fees for the sake of safety.
        //
        if self.total_fees == 0 {
            return [0; 33];
        }

        //
        // if we have a routing path, we calculate the total amount
        // of routing work that it is possible for this transaction
        // to contain (2x the fee).
        //
        // aggregate routing work is only calculated in this function
        // as it is only needed when determining payouts. it should
        // not be confused with total_work which represents the amount
        // of work available in the transaction itself.
        //
        let mut aggregate_routing_work: Currency = self.total_fees;
        let mut routing_work_this_hop: Currency = aggregate_routing_work;
        let mut work_by_hop: Vec<Currency> = vec![];
        work_by_hop.push(aggregate_routing_work);

        for _i in 1..self.path.len() {
            let new_routing_work_this_hop: Currency = routing_work_this_hop / 2;
            aggregate_routing_work += new_routing_work_this_hop;
            routing_work_this_hop = new_routing_work_this_hop;
            work_by_hop.push(aggregate_routing_work);
        }

        //
        // find winning routing node
        //
        let x = U256::from_big_endian(&random_hash);
        let z = U256::from_big_endian(&aggregate_routing_work.to_be_bytes());
        let zy = x.div_mod(z).1;
        let winning_routing_work_in_nolan: Currency = zy.low_u64();

        for i in 0..work_by_hop.len() {
            if winning_routing_work_in_nolan <= work_by_hop[i] {
                return self.path[i].to;
            }
        }

        //
        // we should never reach this
        //
        [0; 33]
    }

    /// Runs when the chain is re-organized
    pub fn on_chain_reorganization(
        &self,
        utxoset: &mut UtxoSet,
        longest_chain: bool,
        block_id: u64,
    ) {
        let mut input_slip_spendable = true;
        let mut output_slip_spendable = false;

        if longest_chain {
            input_slip_spendable = false;
            output_slip_spendable = true;
        }

        self.inputs.iter().for_each(|input| {
            input.on_chain_reorganization(utxoset, longest_chain, input_slip_spendable)
        });
        self.outputs.iter().for_each(|output| {
            output.on_chain_reorganization(utxoset, longest_chain, output_slip_spendable)
        });
    }

    /// [len of inputs - 4 bytes - u32]
    /// [len of outputs - 4 bytes - u32]
    /// [len of message - 4 bytes - u32]
    /// [len of path - 4 bytes - u32]
    /// [signature - 64 bytes - Secp25k1 sig]
    /// [timestamp - 8 bytes - u64]
    /// [transaction type - 1 byte]
    /// [input][input][input]...
    /// [output][output][output]...
    /// [message]
    /// [hop][hop][hop]...
    pub fn serialize_for_net(&self) -> Vec<u8> {
        self.serialize_for_net_with_hop(None)
    }

    pub(crate) fn serialize_for_net_with_hop(&self, opt_hop: Option<Hop>) -> Vec<u8> {
        let mut path_len = self.path.len();
        if opt_hop.is_some() {
            path_len += 1;
        }
        let inputs = self
            .inputs
            .iter()
            .map(|slip| slip.serialize_for_net())
            .collect::<Vec<_>>()
            .concat();
        let outputs = self
            .outputs
            .iter()
            .map(|slip| slip.serialize_for_net())
            .collect::<Vec<_>>()
            .concat();
        let hops = self
            .path
            .iter()
            .map(|hop| hop.serialize_for_net())
            .collect::<Vec<_>>()
            .concat();

        let mut buffer: Vec<u8> = [
            (self.inputs.len() as u32).to_be_bytes().as_slice(),
            (self.outputs.len() as u32).to_be_bytes().as_slice(),
            (self.message.len() as u32).to_be_bytes().as_slice(),
            (path_len as u32).to_be_bytes().as_slice(),
            &self.signature.as_slice(),
            self.timestamp.to_be_bytes().as_slice(),
            self.replaces_txs.to_be_bytes().as_slice(),
            (self.transaction_type as u8).to_be_bytes().as_slice(),
            inputs.as_slice(),
            outputs.as_slice(),
            self.message.as_slice(),
            hops.as_slice(),
        ]
        .concat();

        if !opt_hop.is_none() {
            buffer.extend(opt_hop.unwrap().serialize_for_net());
        }
        buffer
    }

    pub fn serialize_for_signature(&self) -> Vec<u8> {
        // fastest known way that isn't bincode ??

        let inputs = self
            .inputs
            .iter()
            .map(|slip| slip.serialize_input_for_signature())
            .collect::<Vec<_>>()
            .concat();

        let outputs = self
            .outputs
            .iter()
            .map(|slip| slip.serialize_output_for_signature())
            .collect::<Vec<_>>()
            .concat();

        [
            self.timestamp.to_be_bytes().as_slice(),
            inputs.as_slice(),
            outputs.as_slice(),
            (self.replaces_txs as u32).to_be_bytes().as_slice(),
            (self.transaction_type as u32).to_be_bytes().as_slice(),
            self.message.as_slice(),
        ]
        .concat()
    }

    pub fn sign(&mut self, private_key: &SaitoPrivateKey) {
        // we set slip ordinals when signing
        for (i, output) in self.outputs.iter_mut().enumerate() {
            output.slip_index = i as u8;
        }

        let buffer = self.serialize_for_signature();
        let hash_for_signature = hash(&buffer);
        self.hash_for_signature = Some(hash_for_signature);
        self.signature = sign(&buffer, private_key);
    }
    pub fn sign_and_encrypt(&mut self, private_key: &SaitoPrivateKey) {
        todo!()
    }

    pub fn validate(&self, utxoset: &UtxoSet) -> bool {
        // trace!(
        //     "validating transaction : {:?}",
        //     hex::encode(self.hash_for_signature.unwrap())
        // );
        //
        // Fee Transactions are validated in the block class. There can only
        // be one per block, and they are checked by ensuring the transaction hash
        // matches our self-generated safety check. We do not need to validate
        // their input slips as their input slips are records of what to do
        // when reversing/unwinding the chain and have been spent previously.
        //
        if self.transaction_type == TransactionType::Fee {
            return true;
        }

        //
        // User-Sent Transactions
        //
        // most transactions are identifiable by the public_key that
        // has signed their input transaction, but some transactions
        // do not have senders as they are auto-generated as part of
        // the block itself.
        //
        // ATR transactions
        // VIP transactions
        // FEE transactions
        //
        // the first set of validation criteria is applied only to
        // user-sent transactions. validation criteria for the above
        // classes of transactions are further down in this function.
        // at the bottom is the validation criteria applied to ALL
        // transaction types.
        //
        let transaction_type = self.transaction_type;

        if transaction_type != TransactionType::ATR
            && transaction_type != TransactionType::Vip
            && transaction_type != TransactionType::Issuance
        {
            //
            // validate sender exists
            //
            if self.inputs.is_empty() {
                error!("ERROR 582039: less than 1 input in transaction");
                return false;
            }

            //
            // validate signature
            //
            if let Some(hash_for_signature) = &self.hash_for_signature {
                let sig: SaitoSignature = self.signature;
                let public_key: SaitoPublicKey = self.inputs[0].public_key;
                if !verify_signature(hash_for_signature, &sig, &public_key) {
                    error!(
                        "tx verification failed : hash = {:?}, sig = {:?}, pub_key = {:?}",
                        hex::encode(hash_for_signature),
                        hex::encode(sig),
                        hex::encode(public_key)
                    );
                    return false;
                }
            } else {
                //
                // we reach here if we have not already calculated the hash
                // that is checked by the signature. while we could auto-gen
                // it here, we choose to throw an error to raise visibility of
                // unexpected behavior.
                //
                error!("ERROR 757293: there is no hash for signature in a transaction");
                return false;
            }

            //
            // validate routing path sigs
            //
            // a transaction without routing paths is valid, and pays off the
            // sender in the payment lottery. but a transaction with an invalid
            // routing path is fraudulent.
            //
            if !self.validate_routing_path() {
                error!("ERROR 482033: routing paths do not validate, transaction invalid");
                return false;
            }

            // TODO : what happens to tokens when total_out < total_in
            // validate we're not creating tokens out of nothing
            if self.total_out > self.total_in
                && self.transaction_type != TransactionType::Fee
                && self.transaction_type != TransactionType::Vip
            {
                warn!("{:?} in and {:?} out", self.total_in, self.total_out);
                // for _z in self.outputs.iter() {
                //     // info!("{:?} --- ", z.amount);
                // }
                error!("ERROR 802394: transaction spends more than it has available");
                return false;
            }
        }

        //
        // fee transactions
        //
        if transaction_type == TransactionType::Fee {}

        //
        // atr transactions
        //
        if transaction_type == TransactionType::ATR {}

        //
        // normal transactions
        //
        if transaction_type == TransactionType::Normal {}

        //
        // golden ticket transactions
        //
        if transaction_type == TransactionType::GoldenTicket {}

        //
        // vip transactions
        //
        // a special class of transactions that do not pay rebroadcasting
        // fees. these are issued to the early supporters of the Saito
        // project. they carried us and we're going to carry them. thanks
        // for the faith and support.
        //
        if transaction_type == TransactionType::Vip {
            // we should validate that VIP transactions are signed by the
            // public_key associated with the Saito project.
        }

        //
        // all Transactions
        //
        // The following validation criteria apply to all transactions, including
        // those auto-generated and included in blocks.
        //
        //
        // all transactions must have outputs
        //
        if self.outputs.is_empty() {
            error!("ERROR 582039: less than 1 output in transaction");
            return false;
        }

        let inputs_validate = self.validate_against_utxoset(utxoset);
        inputs_validate
    }

    pub fn validate_against_utxoset(&self, utxoset: &UtxoSet) -> bool {
        if self.transaction_type == TransactionType::Fee {
            return true;
        }
        // if inputs exist, they must validate against the UTXOSET
        // if they claim to spend tokens. if the slip has no spendable
        // tokens it will pass this check, which is conducted inside
        // the slip-level validation logic.
        iterate!(self.inputs, 10).all(|input| input.validate(utxoset))
    }

    pub fn validate_routing_path(&self) -> bool {
        self.path.iter().enumerate().all(|(index, hop)| {
            let bytes: Vec<u8> = [self.signature.as_slice(), hop.to.as_slice()].concat();

            // check sig is valid
            if !verify(bytes.as_slice(), &hop.sig, &hop.from) {
                warn!("signature is not valid");
                return false;
            }

            if hop.from == hop.to {
                return false;
            }
            // check path is continuous
            if index > 0 && hop.from != self.path[index - 1].to {
                warn!(
                    "from {:?}: {:?} not matching with previous to {:?}: {:?}. path length = {:?}",
                    index,
                    hex::encode(hop.from),
                    index - 1,
                    hex::encode(self.path[index - 1].to),
                    self.path.len()
                );
                for hop in self.path.iter() {
                    info!(
                        "hop : {:?} --> {:?}",
                        hex::encode(hop.from),
                        hex::encode(hop.to)
                    );
                }
                return false;
            }
            true
        })
    }
    pub fn is_in_path(&self, public_key: &SaitoPublicKey) -> bool {
        if self.is_from(public_key) {
            return true;
        }
        for hop in &self.path {
            if hop.from.eq(public_key) {
                return true;
            }
        }
        false
    }
    pub fn is_from(&self, public_key: &SaitoPublicKey) -> bool {
        iterate!(self.inputs, 10).any(|input| input.public_key.eq(public_key))
    }
    pub fn is_to(&self, public_key: &SaitoPublicKey) -> bool {
        iterate!(self.outputs, 10).any(|slip| slip.public_key.eq(public_key))
    }
}

#[cfg(test)]
mod tests {
    use hex::FromHex;

    use crate::core::data::crypto::generate_keys;

    use super::*;

    #[test]
    fn transaction_new_test() {
        let tx = Transaction::default();
        assert_eq!(tx.timestamp, 0);
        assert_eq!(tx.inputs, vec![]);
        assert_eq!(tx.outputs, vec![]);
        assert_eq!(tx.message, vec![]);
        assert_eq!(tx.transaction_type, TransactionType::Normal);
        assert_eq!(tx.signature, [0; 64]);
        assert_eq!(tx.hash_for_signature, None);
        assert_eq!(tx.total_in, 0);
        assert_eq!(tx.total_out, 0);
        assert_eq!(tx.total_fees, 0);
        assert_eq!(tx.cumulative_fees, 0);
    }

    #[test]
    fn transaction_sign_test() {
        let mut tx = Transaction::default();
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        tx.outputs = vec![Slip::default()];
        tx.sign(&wallet.private_key);

        assert_eq!(tx.outputs[0].slip_index, 0);
        assert_ne!(tx.signature, [0; 64]);
        assert_ne!(tx.hash_for_signature, Some([0; 32]));
    }

    #[test]
    fn serialize_for_signature_test() {
        let tx = Transaction::default();
        assert_eq!(
            tx.serialize_for_signature(),
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]
        );
    }

    #[test]
    fn serialize_for_signature_with_data_test() {
        let mut tx = Transaction::default();
        tx.timestamp = 1637034582666;
        tx.transaction_type = TransactionType::ATR;
        tx.message = vec![
            123, 34, 116, 101, 115, 116, 34, 58, 34, 116, 101, 115, 116, 34, 125,
        ];

        let mut input_slip = Slip::default();
        input_slip.public_key = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        input_slip.block_id = 0;
        input_slip.tx_ordinal = 0;
        input_slip.amount = 123;
        input_slip.slip_index = 10;
        input_slip.slip_type = SlipType::ATR;

        let mut output_slip = Slip::default();
        output_slip.public_key = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        output_slip.block_id = 0;
        output_slip.tx_ordinal = 0;
        output_slip.amount = 345;
        output_slip.slip_index = 23;
        output_slip.slip_type = SlipType::Normal;

        tx.inputs.push(input_slip);
        tx.outputs.push(output_slip);

        // assert_eq!(
        //     tx.serialize_for_signature(),
        //     vec![
        //         0, 0, 1, 125, 38, 221, 98, 138, 220, 246, 204, 235, 116, 113, 127, 152, 195, 247,
        //         35, 148, 89, 187, 54, 253, 205, 143, 53, 14, 237, 191, 204, 251, 235, 247, 192,
        //         176, 22, 31, 205, 139, 204, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123, 10,
        //         1, 220, 246, 204, 235, 116, 113, 127, 152, 195, 247, 35, 148, 89, 187, 54, 253,
        //         205, 143, 53, 14, 237, 191, 204, 251, 235, 247, 192, 176, 22, 31, 205, 139, 204, 0,
        //         0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 89, 23, 0, 0, 0, 0, 1, 0, 0, 0, 3, 123,
        //         34, 116, 101, 115, 116, 34, 58, 34, 116, 101, 115, 116, 34, 125,
        //     ]
        // );
    }

    #[test]
    fn tx_sign_with_data() {
        let mut tx = Transaction::default();
        tx.timestamp = 1637034582666;
        tx.transaction_type = TransactionType::ATR;
        tx.message = vec![
            123, 34, 116, 101, 115, 116, 34, 58, 34, 116, 101, 115, 116, 34, 125,
        ];

        let mut input_slip = Slip::default();
        input_slip.public_key = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        input_slip.block_id = 0;
        input_slip.tx_ordinal = 0;
        input_slip.amount = 123;
        input_slip.slip_index = 10;
        input_slip.slip_type = SlipType::ATR;

        let mut output_slip = Slip::default();
        output_slip.public_key = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        output_slip.block_id = 0;
        output_slip.tx_ordinal = 0;
        output_slip.amount = 345;
        output_slip.slip_index = 23;
        output_slip.slip_type = SlipType::Normal;

        tx.inputs.push(input_slip);
        tx.outputs.push(output_slip);

        tx.sign(
            &<[u8; 32]>::from_hex(
                "854702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235d",
            )
            .unwrap(),
        );

        assert_eq!(tx.signature.len(), 64);
        // assert_eq!(
        //     tx.signature,
        //     [
        //         203, 125, 72, 56, 0, 215, 56, 221, 191, 48, 192, 230, 105, 221, 214, 165, 246, 220,
        //         45, 225, 64, 217, 69, 164, 26, 143, 154, 162, 121, 162, 244, 203, 30, 194, 204,
        //         166, 141, 17, 201, 156, 108, 170, 210, 112, 200, 93, 223, 59, 21, 157, 35, 107,
        //         104, 186, 159, 190, 28, 159, 119, 29, 99, 200, 241, 99
        //     ]
        // );
    }

    #[test]
    fn transaction_generate_cumulative_fees_test() {
        let mut tx = Transaction::default();
        tx.generate_cumulative_fees(1_0000);
        assert_eq!(tx.cumulative_fees, 1_0000);
    }

    #[test]
    fn serialize_for_net_and_deserialize_from_net_test() {
        let mock_input = Slip::default();
        let mock_output = Slip::default();
        let mut mock_hop = Hop::default();

        let mut mock_tx = Transaction::default();
        let mut mock_path: Vec<Hop> = vec![];
        mock_path.push(mock_hop);
        let ctimestamp = 0;

        mock_tx.timestamp = ctimestamp;
        mock_tx.add_input(mock_input);
        mock_tx.add_output(mock_output);
        mock_tx.message = vec![104, 101, 108, 108, 111];
        mock_tx.transaction_type = TransactionType::Normal;
        mock_tx.signature = [1; 64];
        mock_tx.path = mock_path;

        let serialized_tx = mock_tx.serialize_for_net();

        let deserialized_tx = Transaction::deserialize_from_net(&serialized_tx);
        assert_eq!(mock_tx, deserialized_tx);
    }

    // TODO : change the uuid related changes in SLR and add the tx buffer to the test
    #[ignore]
    #[test]
    fn deserialize_test_against_slr() {
        let tx_buffer_txt = "00000001000000010000000300000000dc9f23b0d0feb6609170abddcd5a1de249432b3e6761b8aac39b6e1b5bcb6bef73c1b8af4f394e2b3d983b81ba3e0888feaab092fa1754de8896e22dcfbeb4ec0000017d26dd628a000000010303cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b000000000000007b0a0103cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b00000000000001590000616263";
        let buffer = hex::decode(tx_buffer_txt).unwrap();

        let mut tx = Transaction::deserialize_from_net(&buffer);

        assert_eq!(tx.timestamp, 1637034582666);
        assert_eq!(tx.transaction_type, TransactionType::ATR);

        assert_eq!(hex::encode(tx.signature), "dc9f23b0d0feb6609170abddcd5a1de249432b3e6761b8aac39b6e1b5bcb6bef73c1b8af4f394e2b3d983b81ba3e0888feaab092fa1754de8896e22dcfbeb4ec");
        let public_key: SaitoPublicKey = tx.inputs[0].public_key;
        assert_eq!(
            hex::encode(public_key),
            "03cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610"
        );
        tx.generate(&public_key, 0, 0);
        let sig: SaitoSignature = tx.signature;

        assert_eq!(hex::decode("0000017d26dd628a03cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce7610dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b000000000000007b0a0103cb14a56ddc769932baba62c22773aaf6d26d799b548c8b8f654fb92d25ce76100000000000000000000000000000000000000000000000000000000000000000000000000000015900000000000100000003616263").unwrap()
                   , tx.serialize_for_signature());
        let result = verify(tx.serialize_for_signature().as_slice(), &sig, &public_key);
        assert!(result);
        let result = verify_signature(tx.hash_for_signature.as_ref().unwrap(), &sig, &public_key);
        assert!(result);
    }
}

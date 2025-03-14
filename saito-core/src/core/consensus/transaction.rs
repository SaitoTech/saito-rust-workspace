use ahash::AHashSet;
use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind};

use crate::core::consensus::blockchain::Blockchain;
use log::{debug, error, trace, warn};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use primitive_types::U256;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::consensus::hop::{Hop, HOP_SIZE};
use crate::core::consensus::slip::{Slip, SlipType, SLIP_SIZE};
use crate::core::consensus::wallet::Wallet;
use crate::core::defs::{
    Currency, PrintForLog, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature,
    SaitoUTXOSetKey, Timestamp, UtxoSet, UTXO_KEY_LENGTH,
};
use crate::core::io::network::Network;
use crate::core::util::crypto::{hash, sign, verify, verify_signature};
use crate::iterate;

pub const TRANSACTION_SIZE: usize = 93;

#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Clone, FromPrimitive)]
pub enum TransactionType {
    Normal = 0,
    /// Paying for the network
    Fee = 1,
    GoldenTicket = 2,
    ATR = 3,
    /// VIP transactions deprecated on mainnet
    Vip = 4,
    SPV = 5,
    /// Issues funds for an address at the start of the network
    Issuance = 6,
    BlockStake = 7,
    Bound = 8,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Transaction {
    // the bulk of the consensus transaction data
    pub timestamp: Timestamp,
    pub from: Vec<Slip>,
    pub to: Vec<Slip>,
    // #[serde(with = "serde_bytes")] TODO : check this for performance
    pub data: Vec<u8>,
    pub transaction_type: TransactionType,
    pub txs_replacements: u32,
    #[serde_as(as = "[_; 64]")]
    pub signature: SaitoSignature,
    pub path: Vec<Hop>,

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

impl Display for Transaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "++++++++++++++++++++++++++++++++++++++++++++++++++")?;
        writeln!(
            f,
            "Tx : {{ type : {:?}, data_size : {:?}, timestamp : {:?}, signature : {:?}, hash : {:?}, total_in : {:?}, total_out : {:?}, total_fees : {:?}, total_work_for_me : {:?}, cumulative_fees : {:?}, from slips : count : {:?} }}",
            self.transaction_type,
            self.data.len(),
            self.timestamp,
            self.signature.to_hex(),
            self.hash_for_signature.unwrap_or_default().to_hex(),
            self.total_in,
            self.total_out,
            self.total_fees,
            self.total_work_for_me,
            self.cumulative_fees,
            self.from.len()
        )?;
        if !self.from.is_empty() {
            writeln!(f, "---------------------------------------------")?;
        }
        writeln!(f, " from slips : count : {:?}", self.from.len())?;
        for slip in self.from.iter() {
            writeln!(f, "{}", slip)?;
        }
        if !self.to.is_empty() {
            writeln!(f, "---------------------------------------------")?;
        }
        writeln!(f, " to slips : count : {:?}", self.to.len())?;
        for slip in self.to.iter() {
            writeln!(f, "{}", slip)?;
        }
        if !self.path.is_empty() {
            writeln!(f, "---------------------------------------------")?;
        }
        writeln!(f, " path :  length : {:?}", self.path.len())?;
        for hop in self.path.iter() {
            writeln!(f, "{}", hop)?;
        }
        writeln!(f, "}}")?;
        writeln!(f, "++++++++++++++++++++++++++++++++++++++++++++++++++")
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self {
            timestamp: 0,
            from: vec![],
            to: vec![],
            data: vec![],
            transaction_type: TransactionType::Normal,
            txs_replacements: 1,
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
    pub fn add_from_slip(&mut self, input_slip: Slip) {
        self.from.push(input_slip);
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
    pub fn add_to_slip(&mut self, output_slip: Slip) {
        self.to.push(output_slip);
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
        _force_merge: bool,
        network: Option<&Network>,
        latest_block_id: u64,
        genesis_period: u64,
    ) -> Result<Transaction, Error> {
        Self::create_with_multiple_payments(
            wallet,
            vec![to_public_key],
            vec![with_payment],
            with_fee,
            network,
            latest_block_id,
            genesis_period,
        )
    }

    pub fn create_with_multiple_payments(
        wallet: &mut Wallet,
        mut keys: Vec<SaitoPublicKey>,
        mut payments: Vec<Currency>,
        mut with_fee: Currency,
        network: Option<&Network>,
        latest_block_id: u64,
        genesis_period: u64,
    ) -> Result<Transaction, Error> {
        let total_payment: Currency = payments.iter().sum();
        trace!(
            "generating transaction : payments = {:?}, fee = {:?}",
            total_payment,
            with_fee
        );

        if payments.len() != keys.len() {
            error!("keys and payments provided to the transaction is not similar in count. payments : {:?} keys : {:?}",payments.len(),keys.len());
            return Err(Error::from(ErrorKind::InvalidInput));
        }

        let available_balance = wallet.get_available_balance();

        if with_fee > available_balance {
            with_fee = 0;
        }

        let total_requested = total_payment + with_fee;
        trace!(
            "in generate transaction. available: {} and payment: {} and fee: {}",
            available_balance,
            total_payment,
            with_fee
        );
        if available_balance < total_requested {
            debug!(
                "not enough funds to create transaction. required : {:?} available : {:?}",
                total_requested, available_balance
            );
            return Err(Error::from(ErrorKind::NotFound));
        }

        let mut transaction = Transaction::default();
        if total_requested == 0 {
            let slip = Slip {
                public_key: wallet.public_key,
                amount: 0,
                ..Default::default()
            };
            transaction.add_from_slip(slip);
        } else {
            let (input_slips, output_slips) =
                wallet.generate_slips(total_requested, network, latest_block_id, genesis_period);

            for input in input_slips {
                transaction.add_from_slip(input);
            }
            for output in output_slips {
                transaction.add_to_slip(output);
            }
        }
        for _i in 0..keys.len() {
            let key = keys.pop().unwrap();
            let payment = payments.pop().unwrap();

            let output = Slip {
                public_key: key,
                amount: payment,
                ..Default::default()
            };
            transaction.add_to_slip(output);
        }

        Ok(transaction)
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
    pub fn create_issuance_transaction(
        to_public_key: SaitoPublicKey,
        with_amount: Currency,
    ) -> Transaction {
        trace!("generate issuance transaction : amount = {:?}", with_amount);
        let mut transaction = Transaction::default();
        transaction.transaction_type = TransactionType::Issuance;
        let mut output = Slip::default();
        output.public_key = to_public_key;
        output.amount = with_amount;
        output.slip_type = SlipType::Normal;
        transaction.add_to_slip(output);
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
        to_slip: Slip,
        from_slip: Slip,
    ) -> Transaction {
        debug!(
            "creating rebroadcast transaction \nfrom : {} \nto : {} \ntx_to_rebroadcast: {}",
            from_slip, to_slip, transaction_to_rebroadcast
        );
        let mut transaction = Transaction::default();

        transaction.transaction_type = TransactionType::ATR;

        // if this is the FIRST time we are rebroadcasting, we copy the
        // original transaction into the message field in serialized
        // form. this preserves the original message and its signature
        // in perpetuity.
        //
        // if this is the SECOND or subsequent rebroadcast, we do not
        // copy the ATR tx (no need for a meta-tx) and rather just update
        // the message field with the original transaction (which is
        // by definition already in the previous TX message space.
        if transaction_to_rebroadcast.transaction_type == TransactionType::ATR {
            transaction.data = transaction_to_rebroadcast.data.to_vec();
        } else {
            transaction.data = transaction_to_rebroadcast.serialize_for_net().to_vec();
        }

        transaction.add_from_slip(from_slip);

        // add the output slip
        assert_eq!(to_slip.slip_type, SlipType::ATR);
        transaction.add_to_slip(to_slip);

        transaction.generate_total_fees(0, 0);

        // signature is the ORIGINAL signature. this transaction
        // will fail its signature check and then get analysed as
        // a rebroadcast transaction because of its transaction type.
        transaction.signature = transaction_to_rebroadcast.signature;

        debug!("generated rebroadcast transaction: {}", transaction);

        transaction
    }

    //
    // removes utxoset entries when block is deleted
    //
    pub async fn delete(&self, utxoset: &mut UtxoSet) -> bool {
        self.from.iter().for_each(|input| {
            input.delete(utxoset);
        });
        self.to.iter().for_each(|output| {
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
    pub fn deserialize_from_net(bytes: &[u8]) -> Result<Transaction, Error> {
        // trace!(
        //     "deserializing tx from buffer with length : {:?}",
        //     bytes.len()
        // );
        if bytes.len() < TRANSACTION_SIZE {
            return Err(Error::from(ErrorKind::InvalidData));
        }
        let inputs_len: u32 = u32::from_be_bytes(
            bytes[0..4]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let outputs_len: u32 = u32::from_be_bytes(
            bytes[4..8]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let message_len: usize = u32::from_be_bytes(
            bytes[8..12]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        ) as usize;
        let path_len: usize = u32::from_be_bytes(
            bytes[12..16]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        ) as usize;
        let signature: SaitoSignature = bytes[16..80]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;
        let timestamp: Timestamp = Timestamp::from_be_bytes(
            bytes[80..88]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let replaces_txs = u32::from_be_bytes(
            bytes[88..92]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let transaction_type: TransactionType =
            FromPrimitive::from_u8(bytes[92]).ok_or(Error::from(ErrorKind::InvalidData))?;
        let start_of_inputs = TRANSACTION_SIZE;
        let start_of_outputs = start_of_inputs + inputs_len as usize * SLIP_SIZE;
        let start_of_message = start_of_outputs + outputs_len as usize * SLIP_SIZE;
        let start_of_path = start_of_message + message_len;
        let mut inputs: Vec<Slip> = vec![];
        for n in 0..inputs_len {
            let start_of_data: usize = start_of_inputs + n as usize * SLIP_SIZE;
            let end_of_data: usize = start_of_data + SLIP_SIZE;
            let input = Slip::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec())?;
            inputs.push(input);
        }
        let mut outputs: Vec<Slip> = vec![];
        for n in 0..outputs_len {
            let start_of_data: usize = start_of_outputs + n as usize * SLIP_SIZE;
            let end_of_data: usize = start_of_data + SLIP_SIZE;
            let output = Slip::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec())?;
            outputs.push(output);
        }
        let message = bytes[start_of_message..start_of_message + message_len]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;
        let mut path: Vec<Hop> = vec![];
        for n in 0..path_len {
            let start_of_data: usize = start_of_path + n * HOP_SIZE;
            let end_of_data: usize = start_of_data + HOP_SIZE;
            let hop = Hop::deserialize_from_net(&bytes[start_of_data..end_of_data].to_vec())?;
            path.push(hop);
        }

        let mut transaction = Transaction::default();
        transaction.timestamp = timestamp;
        transaction.from = inputs;
        transaction.to = outputs;
        transaction.data = message;
        transaction.txs_replacements = replaces_txs;
        transaction.transaction_type = transaction_type;
        transaction.signature = signature;
        transaction.path = path;
        Ok(transaction)
    }

    pub fn is_fee_transaction(&self) -> bool {
        self.transaction_type == TransactionType::Fee
    }
    pub fn is_staking_transaction(&self) -> bool {
        self.transaction_type == TransactionType::BlockStake
    }

    pub fn is_atr_transaction(&self) -> bool {
        self.transaction_type == TransactionType::ATR
    }

    pub fn is_normal_transaction(&self) -> bool {
        self.transaction_type == TransactionType::Normal
    }

    pub fn is_golden_ticket(&self) -> bool {
        self.transaction_type == TransactionType::GoldenTicket
    }

    pub fn is_issuance_transaction(&self) -> bool {
        self.transaction_type == TransactionType::Issuance
    }

    // generates
    //
    // when the block is created, block.generate() is called to fill in all the
    // dynamic data related to the block creator. that function in turn calls tx.generate()
    // to ensure that transaction data is generated properly. this includes:
    //
    // tx.hash -> needed to generate merkle root
    // tx.fees -> needed to calculate payouts
    // tx.work -> needed to confirm adequate routing work
    //
    pub fn generate(&mut self, public_key: &SaitoPublicKey, tx_index: u64, block_id: u64) -> bool {
        // ensure hash exists for signing
        self.generate_hash_for_signature();

        // nolan_in, nolan_out, total fees
        self.generate_total_fees(tx_index, block_id);

        // routing work for asserted public_key (creator)
        self.generate_total_work(public_key);

        true
    }

    // calculate cumulative fee share in block
    pub fn generate_cumulative_fees(&mut self, cumulative_fees: Currency) -> Currency {
        self.cumulative_fees = cumulative_fees + self.total_fees;
        self.cumulative_fees
    }

    // calculate total fees in block
    pub fn generate_total_fees(&mut self, tx_index: u64, block_id: u64) {
        // calculate nolan in / out, fees
        // generate utxoset key for every slip
        let nolan_in = self
            .from
            .iter_mut()
            .map(|slip| {
                slip.generate_utxoset_key();
                if let SlipType::Bound = slip.slip_type {
                    // we are not counting the value in Bound slips
                    return 0;
                }
                slip.amount
            })
            .sum::<Currency>();

        let nolan_out = self
            .to
            .iter_mut()
            .enumerate()
            .map(|(index, slip)| {
                if slip.slip_type != SlipType::ATR || slip.slip_type != SlipType::Bound {
                    slip.block_id = block_id;
                    slip.tx_ordinal = tx_index;
                    slip.slip_index = index as u8;
                }
                slip.generate_utxoset_key();
                if let SlipType::Bound = slip.slip_type {
                    // we are not counting the value in Bound slips
                    return 0;
                }
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
    /// calculate cumulative routing work in block
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

        // something is wrong if we are not the last routing node
        let last_hop = &self.path[self.path.len() - 1];
        if last_hop.to.ne(public_key) {
            self.total_work_for_me = 0;
            warn!(
                "tx : {:?} last hop is not current node",
                self.signature.to_hex()
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
                    self.signature.to_hex(),
                    self.path[i - 1].to.to_hex(),
                    self.path[i].from.to_hex()
                );
                return;
            }

            // otherwise halve the work
            let half_of_routing_work: Currency = routing_work_available_to_public_key / 2;
            routing_work_available_to_public_key -= half_of_routing_work;
        }

        self.total_work_for_me = routing_work_available_to_public_key;
    }

    //
    // generate hash used for signing the tx
    //
    pub fn generate_hash_for_signature(&mut self) {
        if let TransactionType::SPV = self.transaction_type {
            self.hash_for_signature = Some(self.signature[0..32].to_vec().try_into().unwrap());
        } else {
            self.hash_for_signature = Some(hash(&self.serialize_for_signature()));
        }
    }

    pub fn get_winning_routing_node(&self, random_hash: SaitoHash) -> SaitoPublicKey {
        //
        // if there are no routing paths, we return the sender of
        // the payment, as they're got all the routing work by
        // definition. this is the edge-case where sending a tx
        // can make you money.
        //
        if self.path.is_empty() {
            return if !self.from.is_empty() {
                self.from[0].public_key
            } else {
                //
                // if there is no routing path and there are no FROM UTXO then this is a
                // fee transaction or issuance transaction. in these cases we choose to
                // graveyard the payout as this should encourage winners to move their
                // payouts, which reinforces the longest chain.
                [0; 33]
            };
        }

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

        unreachable!("winning routing node should've been found before this");
    }

    /// Runs when the chain is re-organized
    pub fn on_chain_reorganization(&self, utxoset: &mut UtxoSet, longest_chain: bool) {
        debug!(
            "tx reorg : {:?} with {} inputs and {} outputs",
            self.signature.to_hex(),
            self.from.len(),
            self.to.len()
        );
        let mut input_slip_spendable = true;
        let mut output_slip_spendable = false;

        if longest_chain {
            input_slip_spendable = false;
            output_slip_spendable = true;
        }

        self.from
            .iter()
            .for_each(|input| input.on_chain_reorganization(utxoset, input_slip_spendable));
        self.to
            .iter()
            .for_each(|output| output.on_chain_reorganization(utxoset, output_slip_spendable));
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
            .from
            .iter()
            .map(|slip| slip.serialize_for_net())
            .collect::<Vec<_>>()
            .concat();
        let outputs = self
            .to
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
            (self.from.len() as u32).to_be_bytes().as_slice(),
            (self.to.len() as u32).to_be_bytes().as_slice(),
            (self.data.len() as u32).to_be_bytes().as_slice(),
            (path_len as u32).to_be_bytes().as_slice(),
            self.signature.as_slice(),
            self.timestamp.to_be_bytes().as_slice(),
            self.txs_replacements.to_be_bytes().as_slice(),
            (self.transaction_type as u8).to_be_bytes().as_slice(),
            inputs.as_slice(),
            outputs.as_slice(),
            self.data.as_slice(),
            hops.as_slice(),
        ]
        .concat();

        if let Some(hop) = opt_hop {
            buffer.extend(hop.serialize_for_net());
        }
        buffer
    }

    /// Returns the size of the serialized transaction buffer without serializing
    pub fn get_serialized_size(&self) -> usize {
        TRANSACTION_SIZE
            + (SLIP_SIZE * self.from.len())
            + (SLIP_SIZE * self.to.len())
            + (HOP_SIZE * self.path.len())
            + self.data.len()
    }

    pub fn serialize_for_signature(&self) -> Vec<u8> {
        // fastest known way that isn't bincode ??

        let inputs = self
            .from
            .iter()
            .map(|slip| slip.serialize_input_for_signature())
            .collect::<Vec<_>>()
            .concat();

        let outputs = self
            .to
            .iter()
            .map(|slip| slip.serialize_output_for_signature())
            .collect::<Vec<_>>()
            .concat();

        [
            self.timestamp.to_be_bytes().as_slice(),
            inputs.as_slice(),
            outputs.as_slice(),
            self.txs_replacements.to_be_bytes().as_slice(),
            (self.transaction_type as u32).to_be_bytes().as_slice(),
            self.data.as_slice(),
        ]
        .concat()
    }

    pub fn sign(&mut self, private_key: &SaitoPrivateKey) {
        // we set slip ordinals when signing
        for (i, output) in self.to.iter_mut().enumerate() {
            output.slip_index = i as u8;
        }

        let buffer = self.serialize_for_signature();
        let hash_for_signature = hash(&buffer);
        self.hash_for_signature = Some(hash_for_signature);
        self.signature = sign(&buffer, private_key);
    }

    pub fn validate(
        &self,
        utxoset: &UtxoSet,
        blockchain: &Blockchain,
        validate_against_utxo: bool,
    ) -> bool {
        // Fee Transactions are validated in the block class. There can only
        // be one per block, and they are checked by ensuring the transaction hash
        // matches our self-generated safety check. We do not need to validate
        // their input slips as their input slips are records of what to do
        // when reversing/unwinding the chain and have been spent previously.
        if self.transaction_type == TransactionType::Fee {
            return true;
        }

        if self.transaction_type == TransactionType::SPV {
            if self.total_fees > 0 {
                error!("ERROR: SPV transaction contains invalid hash");
                return false;
            }

            return true;
        }

        if let TransactionType::BlockStake = self.transaction_type {
            let mut total_stakes = 0;

            for slip in self.to.iter() {
                if !matches!(slip.slip_type, SlipType::BlockStake)
                    && !matches!(slip.slip_type, SlipType::Normal)
                {
                    error!("staking transaction outputs are not staking");
                    return false;
                }

                if matches!(slip.slip_type, SlipType::BlockStake) {
                    total_stakes += slip.amount;
                }
            }

            if total_stakes < blockchain.social_stake_requirement {
                error!(
                    "Not enough funds staked. expected: {:?}, staked: {:?}",
                    blockchain.social_stake_requirement, total_stakes
                );
                return false;
            }

            let mut unique_keys: AHashSet<SaitoUTXOSetKey> = Default::default();

            for slip in self.from.iter() {
                if slip.utxoset_key == [0; UTXO_KEY_LENGTH] {
                    return false;
                }
                if !blockchain.is_slip_unlocked(&slip.utxoset_key) {
                    return false;
                }
                let utxo_slip = Slip::parse_slip_from_utxokey(&slip.utxoset_key).unwrap();
                if utxo_slip.amount != slip.amount {
                    return false;
                }

                unique_keys.insert(slip.utxoset_key);
            }

            if unique_keys.len() != self.from.len() {
                // same utxo is used twice in the transaction
                return false;
            }

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
        // FEE transactions
        //
        // the first set of validation criteria is applied only to
        // user-sent transactions. validation criteria for the above
        // classes of transactions are further down in this function.
        // at the bottom is the validation criteria applied to ALL
        // transaction types.
        let transaction_type = self.transaction_type;

        if transaction_type != TransactionType::ATR && transaction_type != TransactionType::Issuance
        {
            // validate sender exists
            if self.from.is_empty() {
                error!("ERROR 582039: less than 1 input in transaction");
                return false;
            }

            // validate signature
            if let Some(hash_for_signature) = &self.hash_for_signature {
                let sig: SaitoSignature = self.signature;
                let public_key: SaitoPublicKey = self.from[0].public_key;
                if !verify_signature(hash_for_signature, &sig, &public_key) {
                    error!(
                        "tx verification failed : hash = {:?}, sig = {:?}, pub_key = {:?}",
                        hash_for_signature.to_hex(),
                        sig.to_hex(),
                        public_key.to_base58()
                    );
                    return false;
                }
            } else {
                // we reach here if we have not already calculated the hash
                // that is checked by the signature. while we could auto-gen
                // it here, we choose to throw an error to raise visibility of
                // unexpected behavior.
                error!("ERROR 757293: there is no hash for signature in a transaction");
                return false;
            }

            // validate routing path sigs
            //
            // a transaction without routing paths is valid, and pays off the
            // sender in the payment lottery. but a transaction with an invalid
            // routing path is fraudulent.
            if !self.validate_routing_path() {
                error!("ERROR 482033: routing paths do not validate, transaction invalid");
                return false;
            }

            // TODO : what happens to tokens when total_out < total_in
            // validate we're not creating tokens out of nothing
            if self.total_out > self.total_in && self.transaction_type != TransactionType::Fee {
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

        if transaction_type == TransactionType::Bound {
            // ensure there is only one input slip
            if self.from.len() != 1 {
                error!("Bound transaction must have exactly 1 input slip.");
                return false;
            }

            let slip = &self.from[0];

            // first slip shouldnt be bound
            if slip.slip_type == SlipType::Bound {
                error!("Bound transaction input slip cannot be of type Bound.");
                return false;
            }

            // there are exactly 2 or 3 output slips
            if self.to.len() < 2 || self.to.len() > 3 {
                error!("Bound transaction must have exactly 2 or 3 output slips.");
                return false;
            }

            // output contains at least 1 Bound slip and 1 Normal slip.
            let mut has_bound = false;
            let mut has_normal = false;

            for slip in &self.to {
                match slip.slip_type {
                    SlipType::Bound => has_bound = true,
                    SlipType::Normal => has_normal = true,
                    _ => {}
                }
            }

            if !has_bound || !has_normal {
                error!(
                    "Bound transaction must contain at least one Bound slip and one Normal slip."
                );
                return false;
            }

            // the first UTXO_KEY_LENGTH bytes of data match
            // the input slip’s UTXO key
            if self.data.len() < UTXO_KEY_LENGTH {
                error!("Bound transaction data is too short to contain a valid UTXO key.");
                return false;
            }

            let expected_utxokey = &self.from[0].utxoset_key;
            let extracted_utxokey: &[u8] = &self.data[..UTXO_KEY_LENGTH];

            if extracted_utxokey != expected_utxokey {
                error!("Bound transaction data does not start with the correct UTXO key.");
                return false;
            }
        } else {
            if self
                .from
                .iter()
                .any(|slip| slip.slip_type == SlipType::Bound)
            {
                // only bound txs can have bound slips
                return false;
            }
            if self.to.iter().any(|slip| slip.slip_type == SlipType::Bound) {
                // only bound txs can have bound slips
                return false;
            }
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
        if self.to.is_empty() {
            error!("ERROR 582039: less than 1 output in transaction");
            return false;
        }

        return if validate_against_utxo {
            // trace!("validating transaction against utxo ...");
            let inputs_validate = self.validate_against_utxoset(utxoset);
            // trace!("validated transaction against utxo !");
            inputs_validate
        } else {
            true
        };
    }

    pub fn validate_against_utxoset(&self, utxoset: &UtxoSet) -> bool {
        if self.transaction_type == TransactionType::Fee {
            return true;
        }
        // if inputs exist, they must validate against the UTXOSET
        // if they claim to spend tokens. if the slip has no spendable
        // tokens it will pass this check, which is conducted inside
        // the slip-level validation logic.
        iterate!(self.from, 10).all(|input| input.validate(utxoset))
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
                    hop.from.to_hex(),
                    index - 1,
                    self.path[index - 1].to.to_hex(),
                    self.path.len()
                );
                for hop in self.path.iter() {
                    debug!("hop : {:?} --> {:?}", hop.from.to_hex(), hop.to.to_hex());
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
        iterate!(self.from, 10).any(|input| input.public_key.eq(public_key))
    }
    pub fn is_to(&self, public_key: &SaitoPublicKey) -> bool {
        iterate!(self.to, 10).any(|slip| slip.public_key.eq(public_key))
    }
}

#[cfg(test)]
mod tests {
    use crate::core::defs::{PrintForLog, SaitoPrivateKey, SaitoPublicKey};
    use crate::core::util::crypto::generate_keys;

    use super::*;

    #[test]
    fn transaction_new_test() {
        let tx = Transaction::default();
        assert_eq!(tx.timestamp, 0);
        assert_eq!(tx.from, vec![]);
        assert_eq!(tx.to, vec![]);
        assert_eq!(tx.data, Vec::<u8>::new());
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

        tx.to = vec![Slip::default()];
        tx.sign(&wallet.private_key);

        assert_eq!(tx.to[0].slip_index, 0);
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
        tx.timestamp = 1637034582;
        tx.transaction_type = TransactionType::ATR;
        tx.data = vec![
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

        tx.from.push(input_slip);
        tx.to.push(output_slip);

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
        tx.timestamp = 1637034582;
        tx.transaction_type = TransactionType::ATR;
        tx.data = vec![
            123, 34, 116, 101, 115, 116, 34, 58, 34, 116, 101, 115, 116, 34, 125,
        ];

        let mut input_slip = Slip::default();
        input_slip.public_key = SaitoPublicKey::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        input_slip.block_id = 0;
        input_slip.tx_ordinal = 0;
        input_slip.amount = 123;
        input_slip.slip_index = 10;
        input_slip.slip_type = SlipType::ATR;

        let mut output_slip = Slip::default();
        output_slip.public_key = SaitoPublicKey::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        output_slip.block_id = 0;
        output_slip.tx_ordinal = 0;
        output_slip.amount = 345;
        output_slip.slip_index = 23;
        output_slip.slip_type = SlipType::Normal;

        tx.from.push(input_slip);
        tx.to.push(output_slip);

        tx.sign(
            &SaitoPrivateKey::from_hex(
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
        let mock_hop = Hop::default();

        let mut mock_tx = Transaction::default();
        let mut mock_path: Vec<Hop> = vec![];
        mock_path.push(mock_hop);
        let ctimestamp = 0;

        mock_tx.timestamp = ctimestamp;
        mock_tx.add_from_slip(mock_input);
        mock_tx.add_to_slip(mock_output);
        mock_tx.data = vec![104, 101, 108, 108, 111];
        mock_tx.transaction_type = TransactionType::Normal;
        mock_tx.signature = [1; 64];
        mock_tx.path = mock_path;

        let serialized_tx = mock_tx.serialize_for_net();

        let deserialized_tx = Transaction::deserialize_from_net(&serialized_tx).unwrap();
        assert_eq!(mock_tx, deserialized_tx);
    }
}

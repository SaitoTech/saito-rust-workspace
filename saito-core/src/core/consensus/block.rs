use std::convert::TryInto;
use std::io::{Error, ErrorKind};
use std::ops::Rem;
use std::{i128, mem};

use ahash::AHashMap;
use log::{debug, error, info, trace, warn};
use num_derive::FromPrimitive;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::consensus::blockchain::Blockchain;
use crate::core::consensus::burnfee::BurnFee;
use crate::core::consensus::golden_ticket::GoldenTicket;
use crate::core::consensus::hop::HOP_SIZE;
use crate::core::consensus::merkle::MerkleTree;
use crate::core::consensus::slip::{Slip, SlipType, SLIP_SIZE};
use crate::core::consensus::transaction::{Transaction, TransactionType, TRANSACTION_SIZE};
use crate::core::defs::{
    Currency, PrintForLog, SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature,
    SaitoUTXOSetKey, Timestamp, UtxoSet, BLOCK_FILE_EXTENSION, GENESIS_PERIOD,
};
use crate::core::io::storage::Storage;
use crate::core::util::configuration::Configuration;
use crate::core::util::crypto::{hash, sign, verify_signature};
use crate::iterate;

pub const BLOCK_HEADER_SIZE: usize = 237;

//
// object used when generating and validation transactions, containing the
// information that is created selectively according to the transaction fees
// and the optional outbound payments.
//
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct ConsensusValues {
    // expected transaction containing outbound payments
    pub fee_transaction: Option<Transaction>,
    // number of issuance transactions if exists
    pub it_num: u8,
    // index of issuance transactions if exists
    pub it_index: Option<usize>,
    // number of FEE in transactions if exists
    pub ft_num: u8,
    // index of FEE in transactions if exists
    pub ft_index: Option<usize>,
    // number of GT in transactions if exists
    pub gt_num: u8,
    // index of GT in transactions if exists
    pub gt_index: Option<usize>,
    // total fees in block
    pub total_fees: Currency,
    // expected burnfee
    pub expected_burnfee: Currency,
    // expected difficulty
    pub expected_difficulty: u64,
    // rebroadcast txs
    pub rebroadcasts: Vec<Transaction>,
    // number of rebroadcast slips
    pub total_rebroadcast_slips: u64,

    // total amount of SAITO / NOLAN that is rebroadcast through the ATR
    // mechanism. this is the amount BEFORE the ATR fee is deducted and
    // the ATR payout is added, not the amount that ends up being issued
    // in the new block.
    //
    pub total_rebroadcast_nolan: Currency,
    // amount of NOLAN paid by ATR txs
    pub total_rebroadcast_fees_nolan: Currency,
    // amount of NOLAN paid to ATR txs
    pub total_rebroadcast_staking_payouts_nolan: Currency,
    // all ATR txs hashed together
    pub rebroadcast_hash: [u8; 32],
    // dust falling off chain, needs adding to treasury
    pub nolan_falling_off_chain: Currency,
    // amount to add to staker treasury in block
    pub staking_payout: Currency,
    #[serde(skip)]
    // average income
    pub avg_income: Currency,
    // average fee per byte
    pub avg_fee_per_byte: Currency,
    // average nolan rebroadcast per block
    pub avg_nolan_rebroadcast_per_block: Currency,
}

impl ConsensusValues {
    #[allow(clippy::too_many_arguments)]
    pub fn new() -> ConsensusValues {
        ConsensusValues {
            fee_transaction: None,
            it_num: 0,
            it_index: None,
            ft_num: 0,
            ft_index: None,
            gt_num: 0,
            gt_index: None,
            total_fees: 5000,
            expected_burnfee: 1,
            expected_difficulty: 1,
            rebroadcasts: vec![],
            total_rebroadcast_slips: 0,
            total_rebroadcast_nolan: 0,
            total_rebroadcast_fees_nolan: 0,
            total_rebroadcast_staking_payouts_nolan: 0,
            rebroadcast_hash: [0; 32],
            nolan_falling_off_chain: 0,
            staking_payout: 0,
            avg_income: 0,
            avg_fee_per_byte: 0,
            avg_nolan_rebroadcast_per_block: 0,
        }
    }
    pub fn default() -> ConsensusValues {
        ConsensusValues {
            fee_transaction: None,
            it_num: 0,
            it_index: None,
            ft_num: 0,
            ft_index: None,
            gt_num: 0,
            gt_index: None,
            total_fees: 0,
            expected_burnfee: 1,
            expected_difficulty: 1,
            rebroadcasts: vec![],
            total_rebroadcast_slips: 0,
            total_rebroadcast_nolan: 0,
            total_rebroadcast_fees_nolan: 0,
            total_rebroadcast_staking_payouts_nolan: 0,
            rebroadcast_hash: [0; 32],
            nolan_falling_off_chain: 0,
            staking_payout: 0,
            avg_income: 0,
            avg_fee_per_byte: 0,
            avg_nolan_rebroadcast_per_block: 0,
        }
    }
}

///
/// BlockType is a human-readable indicator of the state of the block
/// with particular attention to its state of pruning and the amount of
/// data that is available. It is used by some functions to fetch blocks
/// that require certain types of data, such as the full set of transactions
/// or the UTXOSet
///
/// Hash - a ghost block sent to lite-clients primarily for SPV mode
/// Header - the header of the block without transaction data
/// Full - the full block including transactions and signatures
///
#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Clone, FromPrimitive)]
pub enum BlockType {
    Ghost = 0,
    Header = 1,
    Pruned = 2,
    Full = 3,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Block {
    /// Consensus Level Variables
    ///
    /// these are the variables that are serialized into the block header
    /// and distributed with every block. validating a block requires
    /// confirming that all of these values are correct given the content
    /// in the block itself.
    ///
    pub id: u64,
    pub timestamp: Timestamp,
    pub previous_block_hash: [u8; 32],
    #[serde_as(as = "[_; 33]")]
    pub creator: [u8; 33],
    pub merkle_root: [u8; 32],
    #[serde_as(as = "[_; 64]")]
    pub signature: [u8; 64],
    pub treasury: Currency,
    pub burnfee: Currency,
    pub difficulty: u64,
    pub staking_treasury: Currency,
    pub avg_income: Currency,
    pub avg_fee_per_byte: Currency,
    pub avg_nolan_rebroadcast_per_block: Currency,

    /// Transactions
    ///
    /// these are all of the transactions that are found a full-block.
    /// lite-blocks may only contain subsets of these transactions, which
    /// can be validated independently.
    ///
    pub transactions: Vec<Transaction>,

    /// Non-Consensus Values
    ///
    /// these values are needed when creating or validating a block but are
    /// generated from the block-data and are not included in the block-header
    /// and must be created by running block.generate() which fills in most
    /// of these values.
    ///
    /// the pre_hash is the hash created from all of the contents of this
    /// block. it is then hashed with the previous_block_hash (in header)
    /// to generate the unique hash for this block. this hash is not incl.
    /// in the consensus variables as it can be independently generated.
    ///
    pub pre_hash: SaitoHash,
    /// hash of block, combines pre_hash and previous_block_hash
    pub hash: SaitoHash,
    /// total fees in block from all transactions including ATR txs
    total_fees: Currency,
    /// total routing work in block for block creator
    pub total_work: Currency,
    /// is block on longest chain
    pub in_longest_chain: bool,
    // has golden ticket
    pub has_golden_ticket: bool,
    // has issuance transaction
    pub has_issuance_transaction: bool,
    // issuance transaction index
    pub issuance_transaction_index: u64,
    // has fee transaction
    pub has_fee_transaction: bool,
    // golden ticket index
    pub golden_ticket_index: u64,
    // fee transaction index
    pub fee_transaction_index: u64,
    // number of rebroadcast slips
    pub total_rebroadcast_slips: u64,
    // number of rebroadcast txs
    pub total_rebroadcast_nolan: Currency,
    // all ATR txs hashed together
    pub rebroadcast_hash: [u8; 32],
    // the state of the block w/ pruning etc
    pub block_type: BlockType,
    pub cv: ConsensusValues,
    // vector of staker slips spent this block - used to prevent withdrawals and payouts same block
    #[serde(skip)]
    pub slips_spent_this_block: AHashMap<SaitoUTXOSetKey, u64>,
    #[serde(skip)]
    pub created_hashmap_of_slips_spent_this_block: bool,
    // the peer's connection ID who sent us this block
    #[serde(skip)]
    pub source_connection_id: Option<SaitoPublicKey>,
    #[serde(skip)]
    pub transaction_map: AHashMap<SaitoPublicKey, bool>,
    #[serde(skip)]
    pub force_loaded: bool,
}

impl Block {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Block {
        Block {
            id: 0,
            timestamp: 0,
            previous_block_hash: [0; 32],
            creator: [0; 33],
            merkle_root: [0; 32],
            signature: [0; 64],
            treasury: 0,
            burnfee: 0,
            difficulty: 0,
            staking_treasury: 0,
            avg_income: 0,
            avg_fee_per_byte: 0,
            avg_nolan_rebroadcast_per_block: 0,
            transactions: vec![],
            pre_hash: [0; 32],
            hash: [0; 32],
            total_fees: 0,
            total_work: 0,
            in_longest_chain: false,
            has_golden_ticket: false,
            has_fee_transaction: false,
            has_issuance_transaction: false,
            issuance_transaction_index: 0,
            golden_ticket_index: 0,
            fee_transaction_index: 0,
            total_rebroadcast_slips: 0,
            total_rebroadcast_nolan: 0,
            // must be initialized zeroed-out for proper hashing
            rebroadcast_hash: [0; 32],
            //filename: String::new(),
            block_type: BlockType::Full,
            // hashmap of all SaitoUTXOSetKeys of the slips in the block
            slips_spent_this_block: AHashMap::new(),
            created_hashmap_of_slips_spent_this_block: false,
            source_connection_id: None,
            transaction_map: Default::default(),
            cv: ConsensusValues::default(),
            force_loaded: false,
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.push(tx);
    }

    //
    // returns valid block
    //
    pub async fn create(
        transactions: &mut AHashMap<SaitoSignature, Transaction>,
        previous_block_hash: SaitoHash,
        blockchain: &mut Blockchain,
        current_timestamp: Timestamp,
        public_key: &SaitoPublicKey,
        private_key: &SaitoPrivateKey,
        golden_ticket: Option<Transaction>,
        configs: &(dyn Configuration + Send + Sync),
        storage: &Storage,
    ) -> Block {
        debug!(
            "Block::create : previous block hash : {:?}",
            previous_block_hash.to_hex()
        );

        let mut previous_block_id = 0;
        let mut previous_block_burnfee = 0;
        let mut previous_block_timestamp = 0;
        let mut previous_block_difficulty = 0;
        let mut previous_block_treasury = 0;
        let mut previous_block_staking_treasury = 0;

        if let Some(previous_block) = blockchain.blocks.get(&previous_block_hash) {
            previous_block_id = previous_block.id;
            previous_block_burnfee = previous_block.burnfee;
            previous_block_timestamp = previous_block.timestamp;
            previous_block_difficulty = previous_block.difficulty;
            previous_block_treasury = previous_block.treasury;
            previous_block_staking_treasury = previous_block.staking_treasury;
        }

        let mut block = Block::new();

        let current_burnfee: Currency = BurnFee::calculate_burnfee_for_block(
            previous_block_burnfee,
            current_timestamp,
            previous_block_timestamp,
        );

        block.id = previous_block_id + 1;
        block.previous_block_hash = previous_block_hash;
        block.burnfee = current_burnfee;
        block.timestamp = current_timestamp;
        block.difficulty = previous_block_difficulty;
        block.creator = *public_key;

        if golden_ticket.is_some() {
            debug!("golden ticket found. adding to block.");
            block.transactions.push(golden_ticket.unwrap());
        }
        debug!("adding {:?} transactions to the block", transactions.len());
        block.transactions.reserve(transactions.len());
        let iter = transactions.drain().map(|(_, tx)| tx);

        block.transactions.extend(iter);

        // block.transactions = transactions.drain().collect();
        transactions.clear();

        // update slips_spent_this_block so that we have a record of
        // how many times input slips are spent in this block. we will
        // use this later to ensure there are no duplicates. this include
        // during the fee transaction, so that we cannot pay a staker
        // that is also paid this block otherwise.
        // this will not include the fee transaction or the ATR txs
        // because they have not been added to teh block yet, but they
        // permit us to avoid paying out StakerWithdrawal slips when we
        // generate the fee payment.
        //
        // note -- the FEE TX will not have inputs, but we do not need
        // to add an exception for it because it has not been added to the
        // block yet.
        if !block.created_hashmap_of_slips_spent_this_block {
            debug!("creating hashmap of slips spent this block...");
            for transaction in &block.transactions {
                for input in transaction.from.iter() {
                    block
                        .slips_spent_this_block
                        .entry(input.get_utxoset_key())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                }
                block.created_hashmap_of_slips_spent_this_block = true;
            }
        }

        // contextual values
        let mut cv: ConsensusValues = block.generate_consensus_values(blockchain, storage).await;

        block.cv = cv.clone();

        // ATR transactions
        let rlen = cv.rebroadcasts.len();
        // TODO -- figure out if there is a more efficient solution
        // than iterating through the entire transaction set here.
        let _tx_hashes_generated = cv.rebroadcasts[0..rlen]
            .iter_mut()
            .enumerate()
            .all(|(index, tx)| tx.generate(public_key, index as u64, block.id));
        if rlen > 0 {
            block.transactions.append(&mut cv.rebroadcasts);
        }

        // fee transaction
        //
        // MUST BE ADDED AFTER ATR TRANSACTIONS -- this is the last tx in the block
        //
        if cv.fee_transaction.is_some() {
            debug!("adding fee transaction");
            let mut fee_tx = cv.fee_transaction.unwrap();
            let hash_for_signature: SaitoHash = hash(&fee_tx.serialize_for_signature());
            fee_tx.hash_for_signature = Some(hash_for_signature);
            fee_tx.sign(private_key);
            block.add_transaction(fee_tx);
        }

        // update slips_spent_this_block so that we have a record of
        // how many times input slips are spent in this block. we will
        // use this later to ensure there are no duplicates.
        for transaction in &block.transactions {
            if transaction.transaction_type != TransactionType::Fee {
                for input in transaction.from.iter() {
                    block
                        .slips_spent_this_block
                        .entry(input.get_utxoset_key())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                }
            }
        }
        block.created_hashmap_of_slips_spent_this_block = true;

        // set difficulty
        block.difficulty = cv.expected_difficulty;

        // TODO - we should consider deleting the treasury, if we do not use
        // it as a catch for any tokens removed for blocks where payouts
        // exceed variance permitted and payouts are deducted downwards.
        block.treasury = 0;

        // adjust staking treasury
        if cv.staking_payout != 0 {
            debug!(
                "adding staking payout : {:?} to treasury : {:?}",
                cv.staking_payout, block.staking_treasury
            );
            block.staking_treasury += cv.staking_payout;
        }
        if cv.total_rebroadcast_staking_payouts_nolan != 0
            && block.staking_treasury >= cv.total_rebroadcast_staking_payouts_nolan
        {
            block.staking_treasury -= cv.total_rebroadcast_staking_payouts_nolan;
        }

        // generate merkle root
        let block_merkle_root =
            block.generate_merkle_root(configs.is_browser(), configs.is_spv_mode());
        block.merkle_root = block_merkle_root;

        block.avg_income = cv.avg_income;
        block.avg_fee_per_byte = cv.avg_fee_per_byte;
        block.avg_nolan_rebroadcast_per_block = cv.avg_nolan_rebroadcast_per_block;

        block.generate_pre_hash();
        block.sign(private_key);
        // regenerates pre-hash, etc.
        block.generate();

        block
    }

    //
    // runs when block deleted
    //
    pub async fn delete(&self, utxoset: &mut UtxoSet) -> bool {
        for tx in &self.transactions {
            tx.delete(utxoset).await;
        }
        true
    }

    /// Deserialize from bytes to a Block.
    /// [len of transactions - 4 bytes - u32]
    /// [id - 8 bytes - u64]
    /// [timestamp - 8 bytes - u64]
    /// [previous_block_hash - 32 bytes - SHA 256 hash]
    /// [creator - 33 bytes - Secp25k1 pubkey compact format]
    /// [merkle_root - 32 bytes - SHA 256 hash
    /// [signature - 64 bytes - Secp25k1 sig]
    /// [treasury - 8 bytes - u64]
    /// [staking_treasury - 8 bytes - u64]
    /// [burnfee - 8 bytes - u64]
    /// [difficulty - 8 bytes - u64]
    /// [transaction][transaction][transaction]...
    pub fn deserialize_from_net(bytes: Vec<u8>) -> Result<Block, Error> {
        if bytes.len() < BLOCK_HEADER_SIZE {
            warn!(
                "block buffer is smaller than header length. length : {:?}",
                bytes.len()
            );
            return Err(Error::from(ErrorKind::InvalidData));
        }
        let transactions_len: u32 = u32::from_be_bytes(
            bytes[0..4]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let id: u64 = u64::from_be_bytes(
            bytes[4..12]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let timestamp: Timestamp = Timestamp::from_be_bytes(
            bytes[12..20]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let previous_block_hash: SaitoHash = bytes[20..52]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;
        let creator: SaitoPublicKey = bytes[52..85]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;
        let merkle_root: SaitoHash = bytes[85..117]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;
        let signature: SaitoSignature = bytes[117..181]
            .try_into()
            .or(Err(Error::from(ErrorKind::InvalidData)))?;

        let treasury: Currency = Currency::from_be_bytes(
            bytes[181..189]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let staking_treasury: Currency = Currency::from_be_bytes(
            bytes[189..197]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let burnfee: Currency = Currency::from_be_bytes(
            bytes[197..205]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let difficulty: u64 = u64::from_be_bytes(
            bytes[205..213]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let avg_income: Currency = Currency::from_be_bytes(
            bytes[213..221]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let avg_fee_per_byte: Currency = Currency::from_be_bytes(
            bytes[221..229]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );
        let avg_nolan_rebroadcast_per_block: Currency = Currency::from_be_bytes(
            bytes[229..237]
                .try_into()
                .or(Err(Error::from(ErrorKind::InvalidData)))?,
        );

        let mut transactions = vec![];
        let mut start_of_transaction_data = BLOCK_HEADER_SIZE;
        for _n in 0..transactions_len {
            if bytes.len() < start_of_transaction_data + 16 {
                warn!(
                    "block buffer is invalid to read transaction metadata. length : {:?}, end_of_tx_data : {:?}",
                    bytes.len(),
                    start_of_transaction_data+16
                );
                return Err(Error::from(ErrorKind::InvalidData));
            }
            let inputs_len: u32 = u32::from_be_bytes(
                bytes[start_of_transaction_data..start_of_transaction_data + 4]
                    .try_into()
                    .or(Err(Error::from(ErrorKind::InvalidData)))?,
            );
            let outputs_len: u32 = u32::from_be_bytes(
                bytes[start_of_transaction_data + 4..start_of_transaction_data + 8]
                    .try_into()
                    .or(Err(Error::from(ErrorKind::InvalidData)))?,
            );
            let message_len: usize = u32::from_be_bytes(
                bytes[start_of_transaction_data + 8..start_of_transaction_data + 12]
                    .try_into()
                    .or(Err(Error::from(ErrorKind::InvalidData)))?,
            ) as usize;
            let path_len: usize = u32::from_be_bytes(
                bytes[start_of_transaction_data + 12..start_of_transaction_data + 16]
                    .try_into()
                    .or(Err(Error::from(ErrorKind::InvalidData)))?,
            ) as usize;
            let total_len = inputs_len
                .checked_add(outputs_len)
                .ok_or(Error::from(ErrorKind::InvalidData))?;
            let end_of_transaction_data = start_of_transaction_data
                + TRANSACTION_SIZE
                + (total_len as usize * SLIP_SIZE)
                + message_len
                + path_len * HOP_SIZE;

            if bytes.len() < end_of_transaction_data {
                warn!(
                    "block buffer is invalid to read transaction data. length : {:?}, end of tx data : {:?}, tx_count : {:?}",
                    bytes.len(), end_of_transaction_data, transactions_len
                );
                return Err(Error::from(ErrorKind::InvalidData));
            }
            let transaction = Transaction::deserialize_from_net(
                &bytes[start_of_transaction_data..end_of_transaction_data].to_vec(),
            )?;
            transactions.push(transaction);
            start_of_transaction_data = end_of_transaction_data;
        }

        let mut block = Block::new();
        block.id = id;
        block.timestamp = timestamp;
        block.previous_block_hash = previous_block_hash;
        block.creator = creator;
        block.merkle_root = merkle_root;
        block.signature = signature;
        block.treasury = treasury;
        block.burnfee = burnfee;
        block.difficulty = difficulty;
        block.staking_treasury = staking_treasury;
        block.avg_income = avg_income;
        block.avg_fee_per_byte = avg_fee_per_byte;
        block.avg_nolan_rebroadcast_per_block = avg_nolan_rebroadcast_per_block;
        block.transactions = transactions.to_vec();

        debug!("block.deserialize tx length = {:?}", transactions_len);
        if transactions_len == 0 && !(block.id == 1 && previous_block_hash == [0; 32]) {
            block.block_type = BlockType::Header;
        }

        Ok(block)
    }

    /// downgrade block
    pub async fn downgrade_block_to_block_type(
        &mut self,
        block_type: BlockType,
        _is_browser: bool,
    ) -> bool {
        debug!(
            "downgrading BLOCK_ID {:?} to type : {:?}",
            self.id, block_type
        );

        if self.block_type == block_type {
            return true;
        }

        // if the block type needed is full and we are not,
        // load the block if it exists on disk.
        //
        if block_type == BlockType::Pruned {
            self.transactions = vec![];
            self.block_type = BlockType::Pruned;
            return true;
        }

        false
    }

    //
    // find winning router in block path
    //
    pub fn find_winning_router(&self, random_number: SaitoHash) -> SaitoPublicKey {
        let winner_pubkey: SaitoPublicKey;

        // find winning nolan
        //
        let x = primitive_types::U256::from_big_endian(&random_number);
        // fee calculation should be the same used in block when
        // generating the fee transaction.
        //
        let y = self.total_fees;

        // if there are no fees, payout to burn address
        //
        if y == 0 {
            winner_pubkey = [0; 33];
            return winner_pubkey;
        }

        let z = primitive_types::U256::from_big_endian(&y.to_be_bytes());
        let zy = x.rem(z);
        let winning_nolan: Currency = zy.low_u64();
        // we may need function-timelock object if we need to recreate
        // an ATR transaction to pick the winning routing node.
        let winning_tx_placeholder: Transaction;
        let mut winning_tx: &Transaction;

        // winning TX contains the winning nolan
        //
        // either a fee-paying transaction or an ATR transaction
        //
        winning_tx = &self.transactions[0];
        for transaction in &self.transactions {
            if transaction.cumulative_fees > winning_nolan {
                break;
            }
            winning_tx = &transaction;
        }

        // if winner is atr, we take inside TX
        //
        if winning_tx.transaction_type == TransactionType::ATR {
            let tmptx = winning_tx.data.to_vec();
            winning_tx_placeholder =
                Transaction::deserialize_from_net(&tmptx).expect("buffer to be valid");
            winning_tx = &winning_tx_placeholder;
        }

        // hash random number to pick routing node
        //
        winner_pubkey = winning_tx.get_winning_routing_node(hash(random_number.as_ref()));
        winner_pubkey
    }

    // generate
    //
    // this function generates all of the ancillary data needed to process or
    // validate blocks. this includes the various hashes and other dynamic
    // values that are not provided on the creation of the block object itself
    // but must be calculated from information such as the set of transactions
    // and the presence / absence of golden tickets, etc.
    //
    // we first calculate as much information as we can in parallel before
    // sweeping through the transactions to find out what percentage of the
    // cumulative block fees they contain.
    //
    pub fn generate(&mut self) -> bool {
        // trace!(" ... block.prevalid - pre hash:  {:?}", create_timestamp());

        //
        // ensure block hashes correct
        //
        self.generate_pre_hash();
        self.generate_hash();

        let creator_public_key = &self.creator;

        //
        // allow transactions to generate themselves
        //
        let _transactions_pre_calculated = &self
            .transactions
            .iter_mut()
            .enumerate()
            .all(|(index, tx)| tx.generate(creator_public_key, index as u64, self.id));

        self.generate_transaction_hashmap();

        // trace!(" ... block.prevalid - pst hash:  {:?}", create_timestamp());

        // we need to calculate the cumulative figures AFTER the
        // original figures.
        //
        let mut cumulative_fees = 0;
        let mut total_work = 0;

        let mut has_golden_ticket = false;
        let mut has_fee_transaction = false;
        let mut has_issuance_transaction = false;
        let mut issuance_transaction_index = 0;
        let mut golden_ticket_index = 0;
        let mut fee_transaction_index = 0;

        //
        // we have to do a single sweep through all of the transactions in
        // non-parallel to do things like generate the cumulative order of the
        // transactions in the block for things like work and fee calculations
        // for the lottery.
        //
        // we take advantage of the sweep to perform other pre-validation work
        // like counting up our ATR transactions and generating the hash
        // commitment for all of our rebroadcasts.
        //
        for i in 0..self.transactions.len() {
            let transaction = &mut self.transactions[i];

            cumulative_fees = transaction.generate_cumulative_fees(cumulative_fees);
            total_work += transaction.total_work_for_me;

            //
            // update slips_spent_this_block so that we have a record of
            // how many times input slips are spent in this block. we will
            // use this later to ensure there are no duplicates.
            //
            // we skip the fee transaction as otherwise we have trouble
            // validating the staker slips if we have received a block from
            // someone else -- i.e. we will think the slip is spent in the
            // block when generating the FEE TX to check against the in-block
            // fee tx.
            //
            if !self.created_hashmap_of_slips_spent_this_block
                && transaction.transaction_type != TransactionType::Fee
            {
                for input in transaction.from.iter() {
                    self.slips_spent_this_block
                        .entry(input.get_utxoset_key())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                }
                self.created_hashmap_of_slips_spent_this_block = true;
            }

            //
            // also check the transactions for golden ticket and fees
            //
            match transaction.transaction_type {
                TransactionType::Issuance => {
                    has_issuance_transaction = true;
                    issuance_transaction_index = i as u64;
                }
                TransactionType::Fee => {
                    has_fee_transaction = true;
                    fee_transaction_index = i as u64;
                }
                TransactionType::GoldenTicket => {
                    has_golden_ticket = true;
                    golden_ticket_index = i as u64;
                }
                TransactionType::ATR => {
                    let mut vbytes: Vec<u8> = vec![];
                    vbytes.extend(&self.rebroadcast_hash);
                    vbytes.extend(&transaction.serialize_for_signature());
                    self.rebroadcast_hash = hash(&vbytes);

                    for input in transaction.from.iter() {
                        self.total_rebroadcast_slips += 1;
                        self.total_rebroadcast_nolan += input.amount;
                    }
                }
                _ => {}
            };
        }
        self.has_fee_transaction = has_fee_transaction;
        self.has_golden_ticket = has_golden_ticket;
        self.has_issuance_transaction = has_issuance_transaction;
        self.fee_transaction_index = fee_transaction_index;
        self.golden_ticket_index = golden_ticket_index;
        self.issuance_transaction_index = issuance_transaction_index;

        // update block with total fees
        //
        self.total_fees = cumulative_fees;
        self.total_work = total_work;

        true
    }

    pub fn generate_hash(&mut self) -> SaitoHash {
        let hash_for_hash = hash(&self.serialize_for_hash());
        self.hash = hash_for_hash;
        hash_for_hash
    }

    pub fn generate_merkle_root(&self, is_browser: bool, is_spv: bool) -> SaitoHash {
        debug!("generating the merkle root");

        if self.transactions.is_empty() && (is_browser || is_spv) {
            return self.merkle_root;
        }

        let merkle_root_hash;
        if let Some(tree) = MerkleTree::generate(&self.transactions) {
            merkle_root_hash = tree.get_root_hash();
        } else {
            merkle_root_hash = [0u8; 32];
        }

        debug!(
            "generated the merkle root, Tx Count: {:?}, root = {:?}",
            self.transactions.len(),
            merkle_root_hash.to_hex()
        );

        merkle_root_hash
    }

    //
    // generate_consensus_values examines a block in the context of the blockchain
    // in order to determine the dynamic values that need to be inserted into the
    // block or validated. this includes:
    //
    //   * payouts (golden tickets)
    //   * amount collected into staking treasury
    //   * total fees in block
    //   * difficulty (mining / payout cost)
    //   * total fees in block
    //   * TODO - review generate() and see if we can clean-up the way we
    //     do things so all values are calculated here and merely SET or
    //     confirmed in the validate/generate function.
    //
    // it returns an object from which the values are either assigned to the block
    // or checked to confirm validity.
    //
    pub async fn generate_consensus_values(
        &self,
        blockchain: &Blockchain,
        storage: &Storage,
    ) -> ConsensusValues {
        debug!(
            "generate consensus values for {:?} from : {:?} txs",
            self.hash.to_hex(),
            self.transactions.len()
        );
        let mut cv = ConsensusValues::new();
        let mut num_non_fee_transactions = 0;

        trace!("calculating total fees");

        //
        // we want to minimize the number of times we need to loop through the
        // block, so we use a single loop to add up the total fees in the block
        // and figure out which transactions (if any) are golden ticket transactions
        // issuance transactions (block #1 only) and fee/payout transactions.
        //
        let mut total_tx_size: usize = 0;
        let mut total_fees_in_normal_txs = 0;

        //
        // calculate total fees
        //
        for (index, transaction) in self.transactions.iter().enumerate() {
            if !transaction.is_fee_transaction() {
                cv.total_fees += transaction.total_fees;
            } else {
                //
                // the fee transaction is the last transaction in the block, so we
                // count the number of non-fee transactions in order to know the
                // tx_ordinal of the fee transaction, which is used to figure out
                // what the slips should look like and allow us to simply compare
                // the fee transaction with that in the actual block.
                //
                num_non_fee_transactions += 1;
                cv.ft_num += 1;
                cv.ft_index = Some(index);
            }
            if matches!(transaction.transaction_type, TransactionType::Normal)
                || matches!(transaction.transaction_type, TransactionType::GoldenTicket)
            {
                total_tx_size += transaction.get_serialized_size();
                total_fees_in_normal_txs += transaction.total_fees;
            }

            if transaction.is_golden_ticket() {
                cv.gt_num += 1;
                cv.gt_index = Some(index);
            }

            if transaction.is_issuance_transaction() {
                cv.it_num += 1;
                cv.it_index = Some(index);
            }
        }

        // now that we know the total fees in the block, we can calculate the avg
        // fee per byte in the block as a whole. we need to know this in order to
        // determine how much to charge to ATR rebroadcasting transactions.
        if total_tx_size > 0 {
            cv.avg_fee_per_byte = total_fees_in_normal_txs / total_tx_size as Currency;
        } else {
            cv.avg_fee_per_byte = 0;
        }

        // adjust mining difficulty and income and variance averages
        //
        // this sets avg_nolan_rebroadcast_per_block, but does not update it to reflect the current
        // new status. this permits us to use the value to calculate the ATR payouts in the next
        // step.
        if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
            // burn fee is "block production difficulty" (fee lockup cost)
            cv.expected_burnfee = BurnFee::calculate_burnfee_for_block(
                previous_block.burnfee,
                self.timestamp,
                previous_block.timestamp,
            );

            //
            // difficulty is "mining difficulty" (payout unlock cost)
            //
            // we increase difficulty if two blocks in a row have golden tickets and decrease
            // it if two blocks in a row do not have golden ticket. this targets a difficulty
            // that averages one golden ticket every two blocks.
            //
            cv.expected_difficulty = previous_block.difficulty;
            if previous_block.has_golden_ticket {
                if cv.gt_num > 0 {
                    cv.expected_difficulty += 1;
                }
            } else if cv.gt_num > 0 && cv.expected_difficulty > 0 {
                cv.expected_difficulty -= 1;
            }

            // average income
            //
            // we set these figures according to the values in the previous block,
            // and then adjust them according to the values from this block.
            cv.avg_income = previous_block.avg_income;
            cv.avg_nolan_rebroadcast_per_block = previous_block.avg_nolan_rebroadcast_per_block;

            //
            // average income adjusts gradually over the genesis period
            //
            let adjustment = (previous_block.avg_income as i128 - cv.total_fees as i128)
                / GENESIS_PERIOD as i128;
            cv.avg_income = (cv.avg_income as i128 - adjustment) as Currency;
        } else {
            // if there is no previous block, the burn fee is not adjusted. validation
            // rules will cause the block to fail unless it is the first block. average
            // income is set to whatever the block avg_income is set to.
            cv.avg_income = self.avg_income;
            cv.expected_burnfee = self.burnfee;
            cv.expected_difficulty = self.difficulty;
        }

        // calculate automatic transaction rebroadcasts / ATR / atr
        if self.id > GENESIS_PERIOD + 1 {
            debug!("calculating ATR");

            // which block needs to be rebroadcast?
            let pruned_block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_at_block_id(self.id - GENESIS_PERIOD);

            // load that block
            if let Some(pruned_block) = blockchain.blocks.get(&pruned_block_hash) {
                let result = storage
                    .load_block_from_disk(storage.generate_block_filepath(pruned_block))
                    .await;
                if result.is_err() {
                    error!(
                        "couldn't load block for ATR from disk. block hash : {:?}",
                        pruned_block.hash.to_hex()
                    );
                    error!("{:?}", result.err().unwrap());
                } else {
                    let mut atr_block = result.unwrap();
                    atr_block.generate();
                    assert_ne!(
                        atr_block.block_type,
                        BlockType::Pruned,
                        "block should be fetched fully before this"
                    );
                    // utxos receive their share of the staking_treasury adjusted for
                    // the value of the UTXO that are looping around the blockchain.
                    let expected_utxo_staked = GENESIS_PERIOD * cv.avg_nolan_rebroadcast_per_block;
                    let expected_utxo_payout = if expected_utxo_staked > 0 {
                        self.staking_treasury / expected_utxo_staked
                    } else {
                        0
                    };
                    debug!("expected_utxo_staked : {:?}", expected_utxo_staked);
                    debug!("expected_utxo_payout : {:?}", expected_utxo_payout);

                    // +1 gives us a figure we can multiply any UTXO by in order to
                    // determine the payout for any utxo
                    let expected_atr_multiplier = 1 + expected_utxo_payout;

                    // loop through the block to identify unspend transactions that are
                    // eligible for rebroadcasting.
                    trace!("identifying all unspent txs");
                    for transaction in &atr_block.transactions {
                        trace!("checking tx : {:?}", transaction.signature.to_hex());
                        let mut outputs = vec![];

                        // we want to avoid calculating the size of the transaction or anything more
                        // complicated until we know that we have a transaction that requires
                        // rebroadcasting.
                        for output in transaction.to.iter() {
                            // valid means unspent and non-zero amount
                            if output.validate(&blockchain.utxoset) {
                                outputs.push(output);
                            }
                        }

                        // if we should rebroadcast this transaction, we figure out the payout using
                        // the multiplier we calculated above, and then deduct the ATR fee that is
                        // deducted from the transaction on rebroadcast.
                        if !outputs.is_empty() {
                            let tx_size = transaction.get_serialized_size() as u64;
                            let atr_fee = tx_size * cv.avg_fee_per_byte * 2; // x2 base-fee multiplier because ATR

                            // in the future we can use more complicated logic which attempts to divide
                            // the ATR fee across the UTXO, but for ease of implementation we simply
                            // subtract the fee from every single UTXO that is unspent.
                            //
                            // users who wish to minimize ATR fees should avoid creating transactions
                            // with multiple unspent UTXO .
                            for output in outputs {
                                let atr_payout_for_slip = output.amount * expected_atr_multiplier;
                                let atr_fee_for_slip = atr_fee;

                                if output.amount + atr_payout_for_slip > atr_fee {
                                    cv.total_rebroadcast_nolan += output.amount;
                                    cv.total_rebroadcast_slips += 1;

                                    let mut slip = output.clone();
                                    slip.slip_type = SlipType::ATR;
                                    slip.amount =
                                        output.amount + atr_payout_for_slip - atr_fee_for_slip;

                                    cv.total_rebroadcast_staking_payouts_nolan +=
                                        atr_payout_for_slip;
                                    cv.total_rebroadcast_fees_nolan += atr_fee_for_slip;

                                    // create our ATR rebroadcast transaction
                                    let rebroadcast_tx =
                                        Transaction::create_rebroadcast_transaction(
                                            transaction,
                                            slip,
                                        );

                                    // update cryptographic hash of all ATRs
                                    let mut vbytes: Vec<u8> = vec![];
                                    vbytes.extend(&cv.rebroadcast_hash);
                                    vbytes.extend(&rebroadcast_tx.serialize_for_signature());
                                    cv.rebroadcast_hash = hash(&vbytes);
                                    cv.rebroadcasts.push(rebroadcast_tx);
                                } else {
                                    // this UTXO will be worth less than zero if the atr_payout is
                                    // added and then the atr_fee is deducted. so we do not rebroadcast
                                    // it but collect the dust as a fee paid to the blockchain by the
                                    // utxo with gratitude for its release.
                                    cv.total_rebroadcast_fees_nolan += output.amount;
                                }
                            }
                        } // output
                    }
                    // tx
                    debug!(
                        "transactions : {:?}, rebroadcasts : {:?}",
                        atr_block.transactions.len(),
                        cv.rebroadcasts.len()
                    );
                }
            } // block
        } // if at least 1 genesis period deep

        // we can now adjust the value of avg_nolan_rebroadcast_per_block since
        // we know the total amount of fees that have been rebroadcast in this
        // block.
        //
        // note that we cannot move this above the ATR section as we use the
        // value of this variable (from the last block) to figure out what the
        // ATR payout should be in this block.
        let adjustment = (cv.avg_nolan_rebroadcast_per_block as i128
            - cv.total_rebroadcast_nolan as i128)
            / GENESIS_PERIOD as i128;
        if cv.avg_nolan_rebroadcast_per_block as i128 >= adjustment {
            cv.avg_nolan_rebroadcast_per_block =
                (cv.avg_nolan_rebroadcast_per_block as i128 - adjustment) as Currency;
        }

        debug!(
            "avg_nolan_rebroadcast_per_block : {:?} total_rebroadcast_fees_nolan : {:?} total_rebroadcast_staking_payouts_nolan : {:?} total rebroadcasts : {:?}",
         cv.avg_nolan_rebroadcast_per_block, cv.total_rebroadcast_fees_nolan, cv.total_rebroadcast_staking_payouts_nolan, cv.rebroadcasts.len()
        );

        // calculate payouts
        //
        // every block pays out the PREVIOUS BLOCK(S). how payouts are handled depends
        // on whether this block contains a golden ticket (triggering a payout) and then
        // whether the previous one did or not.
        //
        // [ previous-previous block ] - [ previous block ] - [ this block ]
        //
        // if this block contains a golden ticket:
        //
        //    - 50% of previous block to miner
        //    - 50% of previous block to routing node
        //
        //    if previous block contains a golden ticket:
        //
        //	  - do nothing (previous previous block already paid out)
        //
        //    if previous block does not contain a golden ticket:
        //
        //	  - 50% of previous previous block to staking treasury
        //    	  - 50% of previous previous block to routing node
        //
        // if this block does not contain a golden ticket:
        //
        //    if previous block contains a golden ticket:
        //
        //	  - do nothing
        //
        //    if previous block does not contain a golden ticket
        //
        //        - 50% of previous previous block to staking treasury
        //        - 50% of previous previous block to staking treasury
        //
        // note that all payouts are capped at 150% of the long-term average block fees
        // collected by the network. this is done in order to prevent attackers from
        // gaming the payout lottery in an edge-case attack on the proportionality of
        // payouts to work.
        trace!("calculating payments...");

        // calculating the payouts means filling in this information based on the block
        // content. we use the following section to fill in these values, and then
        // create the block payouts based on the information we recover.
        let mut miner_publickey: SaitoPublicKey = [0; 33];
        let mut miner_payout: Currency = 0;
        let mut router1_payout: Currency = 0;
        let mut router1_publickey: SaitoPublicKey = [0; 33];
        let mut router2_payout: Currency = 0;
        let mut router2_publickey: SaitoPublicKey = [0; 33];
        let mut staking_payout: Currency = 0;

        // if there is a golden ticket
        if let Some(gt_index) = cv.gt_index {
            trace!("!");
            trace!("there is a golden ticket: {:?}", cv.gt_index);

            // we fetch the random number for determining the payouts from the golden ticket
            // in our current block (this block). we will then continually hash it to generate
            // a series of other numbers that can be used to calculate subsequent payouts.
            let golden_ticket: GoldenTicket =
                GoldenTicket::deserialize_from_net(&self.transactions[gt_index].data);
            let mut next_random_number = hash(golden_ticket.random.as_ref());

            // load the previous block
            if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
                //
                // the amount of the payout depends on the fees in the block
                // we will adjust these before creating the final transaction
                // in order to limit against attacks on the lottery mechanism.
                //
                miner_payout = previous_block.total_fees / 2;
                miner_publickey = golden_ticket.public_key;
                router1_payout = previous_block.total_fees - miner_payout;
                router1_publickey = previous_block.find_winning_router(next_random_number);

                trace!("!");
                trace!("there is a miner publickey: {:?}", miner_publickey);

                // iterate our hash 2 times to accomodate for the iteration that was
                // done in order to find the previous winning router.
                next_random_number = hash(next_random_number.as_ref());
                next_random_number = hash(next_random_number.as_ref());

                // if the previous block did NOT contain a golden ticket, we recurse
                // backwards a single block and issue the payouts for that block
                // as well.
                if previous_block.has_golden_ticket {

                    // do nothing
                } else {
                    // we want to pay out the previous_previous_block to a routing node and the staking table
                    if let Some(previous_previous_block) =
                        blockchain.blocks.get(&previous_block.previous_block_hash)
                    {
                        // calculate staker and router payments
                        router2_payout = previous_previous_block.total_fees / 2;
                        router2_publickey =
                            previous_previous_block.find_winning_router(next_random_number);
                        staking_payout = previous_previous_block.total_fees - router2_payout;

                        // finding a router consumes 2 hashes
                        next_random_number = hash(next_random_number.as_slice());
                        next_random_number = hash(next_random_number.as_slice());
                    }
                }
            } else {
                info!(
                    "previous block : {:?} not found for block : {:?} at index : {:?}",
                    self.previous_block_hash.to_hex(),
                    self.hash.to_hex(),
                    self.id
                );
            }

            //
            // if our total payouts are greater than 1.5x the average fee throughput
            // of the blockchain, then we may be experiencing an edge-case attack in
            // which the attacker
            //
            // TODO - get the difference and add it to the staking payouts so that the
            // tokens are not lost but paid out to users.
            //
            //if miner_payout > (previous_block.avg_income as f64 * 1.5) { miner_payout = previous_block.avg_income as f64 * 1.5; }
            //if router1_payout > (previous_block.avg_income as f64 * 1.5) { router1_payout = previous_block.avg_income as f64 * 1.5; }
            //if router2_payout > (previous_block.avg_income as f64 * 1.5) { router2_payout = previous_block.avg_income as f64 * 1.5; }

            //
            // now create fee transactions
            //
            let mut slip_index = 0;
            let mut transaction = Transaction::default();
            transaction.transaction_type = TransactionType::Fee;
            if miner_publickey != [0; 33] {
                let mut output = Slip::default();
                output.public_key = miner_publickey;
                output.amount = miner_payout;
                output.slip_type = SlipType::MinerOutput;
                output.slip_index = slip_index;
                output.tx_ordinal = num_non_fee_transactions + 1;
                output.block_id = self.id;
                transaction.add_to_slip(output.clone());
                slip_index += 1;
            }
            if router1_publickey != [0; 33] {
                let mut output = Slip::default();
                output.public_key = router1_publickey;
                output.amount = router1_payout;
                output.slip_type = SlipType::RouterOutput;
                output.slip_index = slip_index;
                output.tx_ordinal = num_non_fee_transactions + 1;
                output.block_id = self.id;
                transaction.add_to_slip(output.clone());
                slip_index += 1;
            }
            if router2_publickey != [0; 33] {
                let mut output = Slip::default();
                output.public_key = router2_publickey;
                output.amount = router2_payout;
                output.slip_type = SlipType::RouterOutput;
                output.slip_index = slip_index;
                output.tx_ordinal = num_non_fee_transactions + 1;
                output.block_id = self.id;
                transaction.add_to_slip(output.clone());
                slip_index += 1;
            }

            cv.fee_transaction = Some(transaction);
        } else {
            // if no golden ticket, check if previous block was paid out (has golden ticket)
            if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
                if previous_block.has_golden_ticket {

                    // do nothing, already paid out
                } else {
                    // our previous_previous_block is about to disappear, which means
                    // we should collect the funds that would be lost and add them to
                    // the staking treasury.
                    if let Some(previous_previous_block) =
                        blockchain.blocks.get(&previous_block.previous_block_hash)
                    {
                        staking_payout = previous_previous_block.total_fees;
                    }
                }
            }
        }

        cv.staking_payout = staking_payout;

        //
        // TODO - once we start reducing any mining/staking payouts because the fees-in-block
        // are larger than the average, we can put the removed tokens here in order to keep
        // track of them.
        //
        cv.nolan_falling_off_chain = 0;

        cv
    }

    pub fn generate_pre_hash(&mut self) {
        let hash_for_signature = hash(&self.serialize_for_signature());
        self.pre_hash = hash_for_signature;
    }

    pub fn on_chain_reorganization(&mut self, utxoset: &mut UtxoSet, longest_chain: bool) -> bool {
        debug!(
            "block : on chain reorg : {:?} - {:?}",
            self.id,
            self.hash.to_hex()
        );
        for tx in &self.transactions {
            tx.on_chain_reorganization(utxoset, longest_chain);
        }
        self.in_longest_chain = longest_chain;
        true
    }

    // we may want to separate the signing of the block from the setting of the necessary hash
    // we do this together out of convenience only

    pub fn sign(&mut self, private_key: &SaitoPrivateKey) {
        // we set final data
        self.signature = sign(&self.serialize_for_signature(), private_key);
    }

    // serialize the pre_hash and the signature_for_source into a
    // bytes array that can be hashed and then have the hash set.
    pub fn serialize_for_hash(&self) -> Vec<u8> {
        let vbytes: Vec<u8> = [
            self.previous_block_hash.as_slice(),
            self.pre_hash.as_slice(),
        ]
        .concat();
        vbytes
    }

    // serialize major block components for block signature
    // this will manually calculate the merkle_root if necessary
    // but it is advised that the merkle_root be already calculated
    // to avoid speed issues.
    pub fn serialize_for_signature(&self) -> Vec<u8> {
        [
            self.id.to_be_bytes().as_slice(),
            self.timestamp.to_be_bytes().as_slice(),
            self.previous_block_hash.as_slice(),
            self.creator.as_slice(),
            self.merkle_root.as_slice(),
            self.treasury.to_be_bytes().as_slice(),
            self.staking_treasury.to_be_bytes().as_slice(),
            self.burnfee.to_be_bytes().as_slice(),
            self.difficulty.to_be_bytes().as_slice(),
            self.avg_income.to_be_bytes().as_slice(),
            self.avg_fee_per_byte.to_be_bytes().as_slice(),
            self.avg_nolan_rebroadcast_per_block
                .to_be_bytes()
                .as_slice(),
        ]
        .concat()
    }

    /// Serialize a Block for transport or disk.
    /// [len of transactions - 4 bytes - u32]
    /// [id - 8 bytes - u64]
    /// [timestamp - 8 bytes - u64]
    /// [previous_block_hash - 32 bytes - SHA 256 hash]
    /// [creator - 33 bytes - Secp25k1 pubkey compact format]
    /// [merkle_root - 32 bytes - SHA 256 hash
    /// [signature - 64 bytes - Secp25k1 sig]
    /// [treasury - 8 bytes - u64]
    /// [staking_treasury - 8 bytes - u64]
    /// [burnfee - 8 bytes - u64]
    /// [difficulty - 8 bytes - u64]
    /// [avg_income - 8 bytes - u64]
    /// [transaction][transaction][transaction]...
    pub fn serialize_for_net(&self, block_type: BlockType) -> Vec<u8> {
        let mut tx_len_buffer: Vec<u8> = vec![];

        // block headers do not get tx data
        if block_type == BlockType::Header {
            tx_len_buffer.extend(&0_u32.to_be_bytes());
        } else {
            tx_len_buffer.extend(&(self.transactions.iter().len() as u32).to_be_bytes());
        }
        let mut tx_buf = vec![];
        if block_type != BlockType::Header {
            // block headers do not get tx data
            tx_buf = iterate!(self.transactions, 10)
                .map(|transaction| transaction.serialize_for_net())
                .collect::<Vec<_>>()
                .concat();
        }
        let buffer = [
            tx_len_buffer.as_slice(),
            self.id.to_be_bytes().as_slice(),
            self.timestamp.to_be_bytes().as_slice(),
            self.previous_block_hash.as_slice(),
            self.creator.as_slice(),
            self.merkle_root.as_slice(),
            self.signature.as_slice(),
            self.treasury.to_be_bytes().as_slice(),
            self.staking_treasury.to_be_bytes().as_slice(),
            self.burnfee.to_be_bytes().as_slice(),
            self.difficulty.to_be_bytes().as_slice(),
            self.avg_income.to_be_bytes().as_slice(),
            self.avg_fee_per_byte.to_be_bytes().as_slice(),
            self.avg_nolan_rebroadcast_per_block
                .to_be_bytes()
                .as_slice(),
            tx_buf.as_slice(),
        ]
        .concat();

        buffer
    }

    pub async fn update_block_to_block_type(
        &mut self,
        block_type: BlockType,
        storage: &Storage,
        is_browser: bool,
    ) -> bool {
        if self.block_type == block_type {
            return true;
        }

        if block_type == BlockType::Full {
            return self
                .upgrade_block_to_block_type(block_type, storage, is_browser)
                .await;
        }

        if block_type == BlockType::Pruned {
            return self
                .downgrade_block_to_block_type(block_type, is_browser)
                .await;
        }

        false
    }

    // if the block is not at the proper type, try to upgrade it to have the
    // data that is necessary for blocks of that type if possible. if this is
    // not possible, return false. if it is possible, return true once upgraded.
    pub async fn upgrade_block_to_block_type(
        &mut self,
        block_type: BlockType,
        storage: &Storage,
        is_browser: bool,
    ) -> bool {
        debug!(
            "upgrading block : {:?} of type : {:?} to type : {:?}",
            self.hash.to_hex(),
            self.block_type,
            block_type
        );
        if self.block_type == block_type {
            trace!("block type is already {:?}", self.block_type);
            return true;
        }

        //
        // TODO - if the block does not exist on disk, we have to
        // attempt a remote fetch.
        //

        //
        // if the block type needed is full and we are not,
        // load the block if it exists on disk.
        //
        if block_type == BlockType::Full {
            if is_browser {
                return false;
            }
            let new_block = storage
                .load_block_from_disk(storage.generate_block_filepath(&self))
                .await;
            if new_block.is_err() {
                error!(
                    "block not found in disk to upgrade : {:?}",
                    self.hash.to_hex()
                );
                return false;
            }
            let mut new_block = new_block.unwrap();
            let hash_for_signature = hash(&new_block.serialize_for_signature());
            new_block.pre_hash = hash_for_signature;
            let hash_for_hash = hash(&new_block.serialize_for_hash());
            new_block.hash = hash_for_hash;

            // in-memory swap copying txs in block from mempool
            mem::swap(&mut new_block.transactions, &mut self.transactions);
            // transactions need hashes
            self.generate();
            self.block_type = BlockType::Full;

            return true;
        }

        false
    }

    pub fn generate_lite_block(&self, keylist: Vec<SaitoPublicKey>) -> Block {
        debug!(
            "generating lite block for keys : {:?}",
            keylist.iter().map(hex::encode).collect::<Vec<String>>()
        );

        let mut pruned_txs = vec![];
        let mut selected_txs = 0;

        for tx in &self.transactions {
            if tx
                .from
                .iter()
                .any(|slip| keylist.contains(&slip.public_key))
                || tx.to.iter().any(|slip| keylist.contains(&slip.public_key))
                || tx.is_golden_ticket()
            {
                pruned_txs.push(tx.clone());
                selected_txs += 1;
            } else {
                let spv = Transaction {
                    timestamp: tx.timestamp,
                    from: vec![],
                    to: vec![],
                    data: vec![],
                    transaction_type: TransactionType::SPV,
                    txs_replacements: 1,
                    signature: tx.signature,
                    path: vec![],
                    hash_for_signature: tx.hash_for_signature,
                    total_in: 0,
                    total_out: 0,
                    total_fees: 0,
                    total_work_for_me: 0,
                    cumulative_fees: 0,
                };

                pruned_txs.push(spv);
            }
        }

        debug!(
            "selected txs : {} out of {}",
            selected_txs,
            self.transactions.len()
        );

        let mut i = 0;
        while i + 1 < pruned_txs.len() {
            if pruned_txs[i].transaction_type == TransactionType::SPV
                && pruned_txs[i + 1].transaction_type == TransactionType::SPV
                && pruned_txs[i].txs_replacements == pruned_txs[i + 1].txs_replacements
            {
                pruned_txs[i].txs_replacements *= 2;
                let combined_hash = hash(
                    &[
                        pruned_txs[i].hash_for_signature.unwrap(),
                        pruned_txs[i + 1].hash_for_signature.unwrap(),
                    ]
                    .concat(),
                );
                pruned_txs[i].hash_for_signature = Some(combined_hash);
                pruned_txs.remove(i + 1);
            } else {
                i += 2;
            }
        }

        // Create the block with pruned transactions
        let mut block = Block::new();

        block.transactions = pruned_txs;
        block.id = self.id;
        block.timestamp = self.timestamp;
        block.previous_block_hash = self.previous_block_hash;
        block.creator = self.creator;
        block.burnfee = self.burnfee;
        block.difficulty = self.difficulty;
        block.treasury = self.treasury;
        block.staking_treasury = self.staking_treasury;
        block.signature = self.signature;
        block.avg_income = self.avg_income;
        block.hash = self.hash;

        block.merkle_root = self.generate_merkle_root(false, false);

        block
    }

    pub async fn validate(
        &self,
        blockchain: &Blockchain,
        utxoset: &UtxoSet,
        configs: &(dyn Configuration + Send + Sync),
        storage: &Storage,
    ) -> bool {
        // TODO SYNC : Add the code to check whether this is the genesis block and skip validations
        assert!(self.id > 0);
        if configs.is_browser() {
            self.generate_consensus_values(blockchain, storage).await;
            return true;
        }

        if let BlockType::Ghost = self.block_type {
            // block validates since it's a ghost block
            return true;
        }

        // verify block has at least one transaction
        if self.transactions.is_empty() && self.id != 1 && !blockchain.blocks.is_empty() {
            // we check blockchain blocks to make sure #1 block can be created without transactions
            error!("ERROR 424342: block does not validate as it has no transactions",);
            return false;
        }

        // verify block is signed by creator
        if !verify_signature(&self.pre_hash, &self.signature, &self.creator) {
            error!("ERROR 582039: block is not signed by creator or signature does not validate",);
            return false;
        }

        //
        // verify "Consensus Values"
        //
        // consensus data refers to the info in the proposed block that depends
        // on its relationship to other blocks in the chain -- things like the burn
        // fee, the ATR transactions, the golden ticket solution and more.
        //
        // the first step in validating our block is asking our software to calculate
        // what it thinks this data should be. this same function should have been
        // used by the block creator to create this block, so consensus rules allow us
        // to validate it by checking the variables we can see in our block with what
        // they should be given this function.
        //
        let cv = self.generate_consensus_values(blockchain, storage).await;

        //
        // the average number of fees in the block
        //
        if cv.avg_income != self.avg_income {
            error!(
                "block is misreporting its average income. current : {:?} expected : {:?}",
                self.avg_income, cv.avg_income
            );
            return false;
        }

        //
        // validate difficulty
        //
        // difficulty here refers the difficulty of generating a golden ticket
        // for any particular block. this is the difficulty of the mining
        // puzzle that is used for releasing payments.
        //
        // those more familiar with POW and POS should note that "difficulty" of
        // finding a block is represented in the burn fee variable which we have
        // already examined and validated above. producing a block requires a
        // certain amount of golden ticket solutions over-time, so the
        // distinction is in practice less clean.
        //
        if cv.expected_difficulty != self.difficulty {
            error!(
                "ERROR 202392: difficulty is invalid. expected: {:?} vs actual : {:?}",
                cv.expected_difficulty, self.difficulty
            );
            return false;
        }

        //
        // validate burnfee
        //
        // this is the amount of routing work that is needed to produce a block,
        // as derived from the fees in the block and modified by the length of the
        // routing path for each fee-bearing transaction.
        //
        if cv.expected_burnfee != self.burnfee {
            error!(
                "block is misreporting its burnfee. current : {:?} expected : {:?}",
                self.burnfee, cv.expected_burnfee
            );
            return false;
        }
        if cv.avg_fee_per_byte != self.avg_fee_per_byte {
            error!("block is mis-reporting its average fee per byte");
            return false;
        }

        //
        // only block #1 can have an issuance transaction
        //
        if cv.it_num > 0 && self.id > 1 {
            error!("ERROR: blockchain contains issuance after block 1 in chain",);
            return false;
        }

        //
        // many kinds of validation like the burn fee and the golden ticket solution
        // require the existence of the previous block in order to validate. we put all
        // of these validation steps below so they will have access to the previous block
        //
        // if no previous block exists, we are valid only in a limited number of
        // circumstances, such as this being the first block we are adding to our chain.
        //
        if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
            if let BlockType::Ghost = previous_block.block_type {
                return true;
            }

            // staking treasury
            //
            let mut expected_staking_treasury = previous_block.staking_treasury;
            expected_staking_treasury += cv.staking_payout;
            if expected_staking_treasury >= cv.total_rebroadcast_staking_payouts_nolan {
                expected_staking_treasury -= cv.total_rebroadcast_staking_payouts_nolan;
            } else {
                expected_staking_treasury = 0;
            }

            if self.staking_treasury != expected_staking_treasury {
                error!(
                    "ERROR 820391: staking treasury does not validate: {} expected versus {} found",
                    expected_staking_treasury, self.staking_treasury,
                );
                return false;
            }

            // treasury - TODO remove if we are not going to use this. leaving it in
            // for now as it is a good place to put any NOLAN that get removed because
            // of deflationary pressures or attacks that push consensus into not issuing
            // a full payout.
            //
            if self.treasury != 0 {
                error!(
                    "ERROR 123243: treasury is positive. expected : {:?} + {:?} = {:?} actual : {:?} found",
                    previous_block.treasury , 0 ,
                    (previous_block.treasury + 0) ,
                    self.treasury,
                );
                return false;
            }

            // trace!(" ... burn fee in blk validated:  {:?}", create_timestamp());

            //
            // validate routing work required
            //
            // this checks the total amount of fees that need to be burned in this
            // block to be considered valid according to consensus criteria.
            //
            let amount_of_routing_work_needed: Currency =
                BurnFee::return_routing_work_needed_to_produce_block_in_nolan(
                    previous_block.burnfee,
                    self.timestamp,
                    previous_block.timestamp,
                );
            if self.total_work < amount_of_routing_work_needed {
                error!("Error 510293: block lacking adequate routing work from creator. actual : {:?} expected : {:?}",self.total_work, amount_of_routing_work_needed);
                return false;
            }

            // trace!(" ... done routing work required: {:?}", create_timestamp());

            //
            // validate golden ticket
            //
            // the golden ticket is a special kind of transaction that stores the
            // solution to the network-payment lottery in the transaction message
            // field. it targets the hash of the previous block, which is why we
            // tackle it's validation logic here.
            //
            // first we reconstruct the ticket, then calculate that the solution
            // meets our consensus difficulty criteria. note that by this point in
            // the validation process we have already examined the fee transaction
            // which was generated using this solution. If the solution is invalid
            // we find that out now, and it invalidates the block.
            //
            if let Some(gt_index) = cv.gt_index {
                let golden_ticket: GoldenTicket =
                    GoldenTicket::deserialize_from_net(&self.transactions[gt_index].data);
                //
                // we already have a golden ticket, but create a new one pulling the
                // target hash from our previous block to ensure that this ticket is
                // actually valid in the context of our blockchain, and not just
                // internally consistent in the blockchain of the sender.
                //
                let gt = GoldenTicket::create(
                    previous_block.hash,
                    golden_ticket.random,
                    golden_ticket.public_key,
                );
                if !gt.validate(previous_block.difficulty) {
                    error!(
                        "ERROR 801923: Golden Ticket solution does not validate against previous_block_hash : {:?}, difficulty : {:?}, random : {:?}, public_key : {:?} target : {:?}",
                        previous_block.hash.to_hex(),
                        previous_block.difficulty,
                        gt.random.to_hex(),
                        gt.public_key.to_base58(),
                        gt.target.to_hex()
                    );
                    let solution = hash(&gt.serialize_for_net());
                    let solution_num = primitive_types::U256::from_big_endian(&solution);

                    error!(
                        "solution : {:?} leading zeros : {:?}",
                        solution.to_hex(),
                        solution_num.leading_zeros()
                    );
                    return false;
                }
            }
            // trace!(" ... golden ticket: (validated)  {:?}", create_timestamp());
        }

        // trace!(" ... block.validate: (merkle rt) {:?}", create_timestamp());

        //
        // validate atr
        //
        // Automatic Transaction Rebroadcasts are removed programmatically from
        // an earlier block in the blockchain and rebroadcast into the latest
        // block, with a fee being deducted to keep the data on-chain. In order
        // to validate ATR we need to make sure we have the correct number of
        // transactions (and ONLY those transactions!) included in our block.
        //
        // we do this by comparing the total number of ATR slips and nolan
        // which we counted in the generate_metadata() function, with the
        // expected number given the consensus values we calculated earlier.
        //
        if cv.total_rebroadcast_slips != self.total_rebroadcast_slips {
            error!("ERROR 624442: rebroadcast slips total incorrect");
            return false;
        }
        if cv.total_rebroadcast_nolan != self.total_rebroadcast_nolan {
            error!("ERROR 294018: rebroadcast nolan amount incorrect");
            return false;
        }
        if cv.rebroadcast_hash != self.rebroadcast_hash {
            error!("ERROR 123422: hash of rebroadcast transactions incorrect");
            return false;
        }

        //
        // validate merkle root
        //
        if self.merkle_root == [0; 32]
            && self.merkle_root
                != self.generate_merkle_root(configs.is_browser(), configs.is_spv_mode())
        {
            error!("merkle root is unset or is invalid false 1");
            return false;
        }

        // trace!(" ... block.validate: (cv-data)   {:?}", create_timestamp());

        //
        // validate fee transactions
        //
        // because the fee transaction that is created by generate_consensus_values is
        // produced without knowledge of the block in which it will be put, we need to
        // update that transaction with this information prior to hashing it in order
        // for the hash-comparison to work.
        //
        if cv.ft_num > 0 {
            if let (Some(ft_index), Some(fee_transaction_expected)) =
                (cv.ft_index, cv.fee_transaction)
            {
                //
                // no golden ticket? invalid
                //
                if cv.gt_index.is_none() {
                    error!(
                        "ERROR 48203: block appears to have fee transaction without golden ticket"
                    );
                    return false;
                }

                //
                // the fee transaction is hashed to compare it with the one in the block
                //
                let fee_transaction_in_block = self.transactions.get(ft_index).unwrap();
                let hash1 = hash(&fee_transaction_expected.serialize_for_signature());
                let hash2 = hash(&fee_transaction_in_block.serialize_for_signature());
                if hash1 != hash2 {
                    error!(
                        "ERROR 892032: block {} fee transaction doesn't match cv-expected fee transaction",
                        self.id
                    );
                    info!("expected = {:?}", fee_transaction_expected);
                    info!("actual   = {:?}", fee_transaction_in_block);
                    return false;
                }
            }
        }

        // trace!(" ... block.validate: (txs valid) {:?}", create_timestamp());

        //
        // validate transactions
        //
        // validating transactions requires checking that the signatures are valid,
        // the routing paths are valid, and all of the input slips are pointing
        // to spendable tokens that exist in our UTXOSET. this logic is separate
        // from the validation of block-level variables, so is handled in the
        // transaction objects.
        //
        // this is one of the most computationally intensive parts of processing a
        // block which is why we handle it in parallel. the exact logic needed to
        // examine a transaction may depend on the transaction itself, as we have
        // some specific types (Fee / ATR / etc.) that are generated automatically
        // and may have different requirements.
        //
        // the validation logic for transactions is contained in the transaction
        // class, and the validation logic for slips is contained in the slips
        // class. Note that we are passing in a read-only copy of our UTXOSet so
        // as to determine spendability.
        //

        let transactions_valid =
            iterate!(self.transactions, 100).all(|tx: &Transaction| tx.validate(utxoset));

        // let mut transactions_valid = true;
        // for tx in self.transactions.iter() {
        //     if !tx.validate(utxoset) {
        //         transactions_valid = false;
        //         error!(
        //             "tx : {:?} of type : {:?} is not valid",
        //             hex::encode(tx.signature),
        //             tx.transaction_type
        //         );
        //         break;
        //     }
        // }
        if !transactions_valid {
            error!("ERROR 579128: Invalid transactions found, block validation failed");
        }

        transactions_valid
    }

    pub fn generate_transaction_hashmap(&mut self) {
        if !self.transaction_map.is_empty() {
            return;
        }
        for tx in self.transactions.iter() {
            for slip in tx.from.iter() {
                // TODO : use a hashset instead ??
                self.transaction_map.insert(slip.public_key, true);
            }
            for slip in tx.to.iter() {
                self.transaction_map.insert(slip.public_key, true);
            }
        }
    }
    pub fn has_keylist_txs(&self, keylist: Vec<SaitoPublicKey>) -> bool {
        for key in keylist {
            if self.transaction_map.contains_key(&key) {
                return true;
            }
        }
        false
    }

    pub fn get_file_name(&self) -> String {
        let timestamp = self.timestamp;
        let block_hash = self.hash;

        timestamp.to_string() + "-" + block_hash.to_hex().as_str() + BLOCK_FILE_EXTENSION
    }
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;
    use futures::future::join_all;
    use hex::FromHex;
    use log::info;

    use crate::core::consensus::block::{Block, BlockType};
    use crate::core::consensus::merkle::MerkleTree;
    use crate::core::consensus::slip::{Slip, SlipType};
    use crate::core::consensus::transaction::{Transaction, TransactionType};
    use crate::core::consensus::wallet::Wallet;
    use crate::core::defs::{Currency, SaitoHash, SaitoPrivateKey, SaitoPublicKey, GENESIS_PERIOD};
    use crate::core::io::storage::Storage;
    use crate::core::util::crypto::{generate_keys, verify_signature};
    use crate::core::util::test::test_manager::test::TestManager;
    use crate::lock_for_read;

    #[test]
    fn block_new_test() {
        let block = Block::new();
        assert_eq!(block.id, 0);
        assert_eq!(block.timestamp, 0);
        assert_eq!(block.previous_block_hash, [0; 32]);
        assert_eq!(block.creator, [0; 33]);
        assert_eq!(block.merkle_root, [0; 32]);
        assert_eq!(block.signature, [0; 64]);
        assert_eq!(block.treasury, 0);
        assert_eq!(block.burnfee, 0);
        assert_eq!(block.difficulty, 0);
        assert_eq!(block.transactions, vec![]);
        assert_eq!(block.pre_hash, [0; 32]);
        assert_eq!(block.hash, [0; 32]);
        assert_eq!(block.total_fees, 0);
        assert_eq!(block.total_work, 0);
        assert_eq!(block.in_longest_chain, false);
        assert_eq!(block.has_golden_ticket, false);
        assert_eq!(block.has_issuance_transaction, false);
        assert_eq!(block.issuance_transaction_index, 0);
        assert_eq!(block.has_fee_transaction, false);
        assert_eq!(block.fee_transaction_index, 0);
        assert_eq!(block.golden_ticket_index, 0);
        assert_eq!(block.total_rebroadcast_slips, 0);
        assert_eq!(block.total_rebroadcast_nolan, 0);
        assert_eq!(block.rebroadcast_hash, [0; 32]);
        assert_eq!(block.block_type, BlockType::Full);
        assert_eq!(block.slips_spent_this_block, AHashMap::new());
        assert_eq!(block.created_hashmap_of_slips_spent_this_block, false);
        assert_eq!(block.source_connection_id, None);
    }

    #[test]
    fn block_generate_test() {
        let mut block = Block::new();
        block.generate();

        // block hashes should have updated
        assert_ne!(block.pre_hash, [0; 32]);
        assert_ne!(block.hash, [0; 32]);
        assert_ne!(block.pre_hash, [0; 32]);
        assert_ne!(block.hash, [0; 32]);
        assert_eq!(block.pre_hash, block.pre_hash);
        assert_eq!(block.hash, block.hash);
    }

    #[test]
    fn block_signature_test() {
        let mut block = Block::new();

        block.id = 10;
        block.timestamp = 1637034582;
        block.previous_block_hash = <SaitoHash>::from_hex(
            "bcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b",
        )
        .unwrap();
        block.merkle_root = <SaitoHash>::from_hex(
            "ccf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b",
        )
        .unwrap();
        block.creator = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        block.burnfee = 50000000;
        block.difficulty = 0;
        block.treasury = 0;
        block.staking_treasury = 0;
        block.signature = <[u8; 64]>::from_hex("c9a6c2d0bf884be6933878577171a3c8094c2bf6e0bc1b4ec3535a4a55224d186d4d891e254736cae6c0d2002c8dfc0ddfc7fcdbe4bc583f96fa5b273b9d63f4").unwrap();

        let serialized_body = block.serialize_for_signature();
        assert_eq!(serialized_body.len(), 169);

        block.creator = <SaitoPublicKey>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();

        block.sign(
            &<SaitoHash>::from_hex(
                "854702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235d",
            )
            .unwrap(),
        );

        assert_eq!(block.signature.len(), 64);
    }

    #[test]
    fn block_serialization_and_deserialization_test() {
        // pretty_env_logger::init();
        let mock_input = Slip::default();
        let mock_output = Slip::default();

        let mut mock_tx = Transaction::default();
        mock_tx.timestamp = 0;
        mock_tx.add_from_slip(mock_input.clone());
        mock_tx.add_to_slip(mock_output.clone());
        mock_tx.data = vec![104, 101, 108, 111];
        mock_tx.transaction_type = TransactionType::Normal;
        mock_tx.signature = [1; 64];

        let mut mock_tx2 = Transaction::default();
        mock_tx2.timestamp = 0;
        mock_tx2.add_from_slip(mock_input);
        mock_tx2.add_to_slip(mock_output);
        mock_tx2.data = vec![];
        mock_tx2.transaction_type = TransactionType::Normal;
        mock_tx2.signature = [2; 64];

        let timestamp = 0;

        let mut block = Block::new();
        block.id = 1;
        block.timestamp = timestamp;
        block.previous_block_hash = [1; 32];
        block.creator = [2; 33];
        block.merkle_root = [3; 32];
        block.signature = [4; 64];
        block.treasury = 1_000_000;
        block.burnfee = 2;
        block.difficulty = 3;
        block.transactions = vec![mock_tx, mock_tx2];

        let serialized_block = block.serialize_for_net(BlockType::Full);
        let deserialized_block = Block::deserialize_from_net(serialized_block).unwrap();

        let serialized_block_header = block.serialize_for_net(BlockType::Header);
        let deserialized_block_header =
            Block::deserialize_from_net(serialized_block_header).unwrap();

        assert_eq!(
            block.serialize_for_net(BlockType::Full),
            deserialized_block.serialize_for_net(BlockType::Full)
        );

        assert_eq!(deserialized_block.id, 1);
        assert_eq!(deserialized_block.timestamp, timestamp);
        assert_eq!(deserialized_block.previous_block_hash, [1; 32]);
        assert_eq!(deserialized_block.creator, [2; 33]);
        assert_eq!(deserialized_block.merkle_root, [3; 32]);
        assert_eq!(deserialized_block.signature, [4; 64]);
        assert_eq!(deserialized_block.treasury, 1_000_000);
        assert_eq!(deserialized_block.burnfee, 2);
        assert_eq!(deserialized_block.difficulty, 3);

        assert_eq!(
            deserialized_block_header.serialize_for_net(BlockType::Full),
            deserialized_block.serialize_for_net(BlockType::Header)
        );

        assert_eq!(deserialized_block_header.id, 1);
        assert_eq!(deserialized_block_header.timestamp, timestamp);
        assert_eq!(deserialized_block_header.previous_block_hash, [1; 32]);
        assert_eq!(deserialized_block_header.creator, [2; 33]);
        assert_eq!(deserialized_block_header.merkle_root, [3; 32]);
        assert_eq!(deserialized_block_header.signature, [4; 64]);
        assert_eq!(deserialized_block_header.treasury, 1_000_000);
        assert_eq!(deserialized_block_header.burnfee, 2);
        assert_eq!(deserialized_block_header.difficulty, 3);
    }

    #[test]
    fn block_sign_and_verify_test() {
        let keys = generate_keys();

        let wallet = Wallet::new(keys.1, keys.0);
        let mut block = Block::new();
        block.creator = wallet.public_key;
        block.generate();
        block.sign(&wallet.private_key);
        block.generate_hash();

        assert_eq!(block.creator, wallet.public_key);
        assert_eq!(
            verify_signature(&block.pre_hash, &block.signature, &block.creator),
            true
        );
        assert_ne!(block.hash, [0; 32]);
        assert_ne!(block.signature, [0; 64]);
    }

    #[test]
    fn block_merkle_root_test() {
        let mut block = Block::new();
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        let transactions: Vec<Transaction> = (0..5)
            .into_iter()
            .map(|_| {
                let mut transaction = Transaction::default();
                transaction.sign(&wallet.private_key);
                transaction
            })
            .collect();

        block.transactions = transactions;
        block.merkle_root = block.generate_merkle_root(false, false);

        assert_eq!(block.merkle_root.len(), 32);
        assert_ne!(block.merkle_root, [0; 32]);
    }

    #[tokio::test]
    #[serial_test::serial]
    // downgrade and upgrade a block with transactions
    async fn block_downgrade_upgrade_test() {
        let mut t = TestManager::new();
        let wallet_lock = t.wallet_lock.clone();
        let mut block = Block::new();
        let transactions = join_all((0..5).into_iter().map(|_| async {
            let mut transaction = Transaction::default();
            let wallet = lock_for_read!(wallet_lock, LOCK_ORDER_WALLET);

            transaction.sign(&wallet.private_key);
            transaction
        }))
        .await
        .to_vec();

        block.transactions = transactions;
        block.generate();

        // save to disk
        t.storage.write_block_to_disk(&mut block).await;

        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.block_type, BlockType::Full);

        let serialized_full_block = block.serialize_for_net(BlockType::Full);
        block
            .update_block_to_block_type(BlockType::Pruned, &mut t.storage, false)
            .await;

        assert_eq!(block.transactions.len(), 0);
        assert_eq!(block.block_type, BlockType::Pruned);

        block
            .update_block_to_block_type(BlockType::Full, &mut t.storage, false)
            .await;

        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.block_type, BlockType::Full);
        assert_eq!(
            serialized_full_block,
            block.serialize_for_net(BlockType::Full)
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn generate_lite_block_test() {
        let mut t = TestManager::new();

        // test blocks with transactions
        // Block 1
        // perform transaction to wallet public key
        t.initialize(100, 100000).await;
        let block1 = t.get_latest_block().await;
        let public_key: SaitoPublicKey;
        {
            let wallet = lock_for_read!(t.wallet_lock, LOCK_ORDER_WALLET);
            public_key = wallet.public_key;
        }
        let lite_block = block1.generate_lite_block(vec![public_key]);
        assert_eq!(lite_block.signature, block1.signature);

        // Second Block
        // perform a transaction to public key
        let public_key =
            Storage::decode_str("s8oFPjBX97NC2vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR").unwrap();
        let mut to_public_key: SaitoPublicKey = [0u8; 33];
        to_public_key.copy_from_slice(&public_key);
        t.transfer_value_to_public_key(to_public_key.clone(), 500, block1.timestamp + 120000)
            .await
            .unwrap();
        let block2 = t.get_latest_block().await;
        let lite_block2 = block2.generate_lite_block(vec![to_public_key]);
        assert_eq!(lite_block2.signature, block2.clone().signature);

        // block 3
        // Perform no transacton
        let block2_hash = block2.hash;
        let mut block3 = t
            .create_block(
                block2_hash,               // hash of parent block
                block2.timestamp + 120000, // timestamp
                0,                         // num transactions
                0,                         // amount
                0,                         // fee
                true,                      // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes
        dbg!(block3.id);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn verify_spv_transaction_in_lite_block_test() {
        let mut t = TestManager::new();

        // Initialize the test manager
        t.initialize(100, 100000).await;

        // Retrieve the latest block
        let mut block = t.get_latest_block().await;

        // Get the wallet's keys
        let private_key: SaitoPrivateKey;
        let public_key: SaitoPublicKey;
        {
            let wallet = lock_for_read!(t.wallet_lock, LOCK_ORDER_WALLET);

            public_key = wallet.public_key;
            private_key = wallet.private_key;
        }

        // Set up the recipient's public key
        let _public_key = "27UK2MuBTdeARhYp97XBnCovGkEquJjkrQntCgYoqj6GC";
        let mut to_public_key: SaitoPublicKey = [0u8; 33];

        // Create VIP transactions
        for _ in 0..50 {
            let public_key_ = Storage::decode_str(&_public_key).unwrap();
            to_public_key.copy_from_slice(&public_key_);
            let mut tx = Transaction::default();
            tx.transaction_type = TransactionType::Normal;
            let mut output = Slip::default();
            output.public_key = to_public_key;
            output.amount = 0;
            output.slip_type = SlipType::Normal;
            tx.add_to_slip(output);
            tx.generate(&public_key, 0, 0);
            tx.sign(&private_key);
            // dbg!(&tx.hash_for_signature);
            block.add_transaction(tx);
        }

        {
            let configs = lock_for_read!(t.configs, LOCK_ORDER_CONFIGS);
            block.merkle_root =
                block.generate_merkle_root(configs.is_browser(), configs.is_spv_mode());
        }

        // Generate and sign the block
        block.generate();
        block.sign(&private_key);

        // Generate a lite block from the full block, using the public keys for SPV transactions
        let lite_block = block.generate_lite_block(vec![public_key]);

        // Find the SPV transaction in the lite block
        let _spv_tx = lite_block
            .transactions
            .iter()
            .find(|&tx| tx.transaction_type == TransactionType::SPV)
            .expect("No SPV transaction found")
            .clone();

        // Generate a Merkle tree from the block transactions
        let _merkle_tree = MerkleTree::generate(&lite_block.transactions)
            .expect("Failed to generate Merkle tree for block");

        dbg!(
            lite_block.generate_merkle_root(false, false),
            block.generate_merkle_root(false, false)
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn avg_fee_per_byte_test() {
        // pretty_env_logger::init();
        let mut t = TestManager::new();

        // Initialize the test manager
        t.initialize(250, 10_000_000).await;

        let latest_block = t.get_latest_block().await;

        let mut block = t
            .create_block(
                latest_block.hash,
                latest_block.timestamp + 40000,
                100,
                1000,
                1_000_000,
                true,
            )
            .await;

        block.generate();

        let mut tx_size = 0;
        let mut total_fees = 0;

        for tx in &block.transactions {
            if !tx.is_fee_transaction() {
                tx_size += tx.serialize_for_net().len();
                total_fees += tx.total_fees;
            }
        }

        info!(
            "avg fee per byte 1: {:?} total fees = {:?} tx size = {:?} tx count = {:?}",
            block.avg_fee_per_byte,
            total_fees,
            tx_size,
            block.transactions.len()
        );
        assert_eq!(block.avg_fee_per_byte, total_fees / tx_size as Currency);

        let mut block = t
            .create_block(
                latest_block.hash,
                latest_block.timestamp + 140000,
                100,
                1000,
                1_000_000,
                false,
            )
            .await;

        block.generate();

        let mut tx_size = 0;
        let mut total_fees = 0;

        for tx in &block.transactions {
            if !tx.is_fee_transaction() {
                tx_size += tx.serialize_for_net().len();
                total_fees += tx.total_fees;
            }
        }
        info!(
            "avg fee per byte 2: {:?} total fees = {:?} tx size = {:?} tx count = {:?}",
            block.avg_fee_per_byte,
            total_fees,
            tx_size,
            block.transactions.len()
        );
        assert_eq!(block.avg_fee_per_byte, total_fees / tx_size as Currency);
    }

    #[ignore]
    #[tokio::test]
    #[serial_test::serial]
    async fn atr_test() {
        // pretty_env_logger::init();

        // create test manager
        let mut t = TestManager::new();

        t.initialize(100, 10000).await;

        // check if epoch length is 10
        assert_eq!(GENESIS_PERIOD, 10, "Genesis period is not 10");

        // create 10 blocks
        for _i in 0..GENESIS_PERIOD {
            let mut block = t
                .create_block(
                    t.latest_block_hash,
                    t.get_latest_block().await.timestamp + 10_000,
                    10,
                    100,
                    10,
                    true,
                )
                .await;
            block.generate();
            t.add_block(block).await;
        }

        // check consensus values for 10th block
        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;

        let latest_block = t.get_latest_block().await;
        let cv = latest_block.cv;

        println!("cv : {:?}", cv);

        // add 11th block
        let mut block = t
            .create_block(
                t.latest_block_hash,
                t.get_latest_block().await.timestamp + 10_000,
                10,
                100,
                10,
                true,
            )
            .await;
        block.generate();
        t.add_block(block).await;

        // check consensus values for 11th block
        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;

        let latest_block = t.get_latest_block().await;
        let cv = latest_block.cv;

        println!("cv2 : {:?}", cv);
    }
}

use std::convert::TryInto;
use std::{mem, sync::Arc};

use ahash::AHashMap;
use bigint::uint::U256;
use log::{debug, error, info, trace};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::common::defs::{
    SaitoHash, SaitoPrivateKey, SaitoPublicKey, SaitoSignature, SaitoUTXOSetKey, UtxoSet,
};
use crate::core::data::blockchain::{Blockchain, GENESIS_PERIOD, MAX_STAKER_RECURSION};
use crate::core::data::burnfee::BurnFee;
use crate::core::data::crypto::{hash, sign, verify};
use crate::core::data::golden_ticket::GoldenTicket;
use crate::core::data::hop::HOP_SIZE;
use crate::core::data::merkle::MerkleTree;
use crate::core::data::slip::{Slip, SlipType, SLIP_SIZE};
use crate::core::data::storage::Storage;
use crate::core::data::transaction::{Transaction, TransactionType, TRANSACTION_SIZE};
use crate::core::data::wallet::Wallet;

pub const BLOCK_HEADER_SIZE: usize = 245;

//
// object used when generating and validation transactions, containing the
// information that is created selectively according to the transaction fees
// and the optional outbound payments.
//
#[derive(PartialEq, Debug, Clone)]
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
    pub total_fees: u64,
    // expected difficulty
    pub expected_difficulty: u64,
    // rebroadcast txs
    pub rebroadcasts: Vec<Transaction>,
    // number of rebroadcast slips
    pub total_rebroadcast_slips: u64,
    // number of rebroadcast txs
    pub total_rebroadcast_nolan: u64,
    // number of rebroadcast fees in block
    pub total_rebroadcast_fees_nolan: u64,
    // number of rebroadcast staking payouts in block
    pub total_rebroadcast_staking_payouts_nolan: u64,
    // all ATR txs hashed together
    pub rebroadcast_hash: [u8; 32],
    // dust falling off chain, needs adding to treasury
    pub nolan_falling_off_chain: u64,
    // staker treasury -> amount to add
    pub staking_treasury: i64,
    // block payout
    pub block_payout: Vec<BlockPayout>,
    // average income
    pub avg_income: u64,
    // average variance
    pub avg_variance: u64,
    // average atr income
    pub avg_atr_income: u64,
    // average atr variance
    pub avg_atr_variance: u64,
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
            total_fees: 0,
            expected_difficulty: 1,
            rebroadcasts: vec![],
            total_rebroadcast_slips: 0,
            total_rebroadcast_nolan: 0,
            total_rebroadcast_fees_nolan: 0,
            total_rebroadcast_staking_payouts_nolan: 0,
            // must be initialized zeroed-out for proper hashing
            rebroadcast_hash: [0; 32],
            nolan_falling_off_chain: 0,
            staking_treasury: 0,
            block_payout: vec![],
            avg_income: 0,
            avg_variance: 0,
            avg_atr_income: 0,
            avg_atr_variance: 0,
        }
    }
}

//
// The BlockPayout object is returned by each block to report who
// receives the payment from the block. It is included in the
// consensus_values so that the fee transaction can be generated
// and validated.
//
#[derive(PartialEq, Debug, Clone)]
pub struct BlockPayout {
    pub miner: SaitoPublicKey,
    pub router: SaitoPublicKey,
    pub miner_payout: u64,
    pub router_payout: u64,
    pub staking_treasury: i64,
    pub random_number: SaitoHash,
}

impl BlockPayout {
    #[allow(clippy::too_many_arguments)]
    pub fn new() -> BlockPayout {
        BlockPayout {
            miner: [0; 33],
            router: [0; 33],
            miner_payout: 0,
            router_payout: 0,
            staking_treasury: 0,
            random_number: [0; 32],
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
#[derive(Serialize, Deserialize, Debug, Copy, PartialEq, Clone)]
pub enum BlockType {
    Ghost,
    Header,
    Pruned,
    Full,
}

#[serde_with::serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Block {
    /// Consensus Level Variables
    pub id: u64,
    pub(crate) timestamp: u64,
    pub(crate) previous_block_hash: [u8; 32],
    #[serde_as(as = "[_; 33]")]
    pub(crate) creator: [u8; 33],
    pub(crate) merkle_root: [u8; 32],
    #[serde_as(as = "[_; 64]")]
    pub signature: [u8; 64],
    pub(crate) treasury: u64,
    pub(crate) burnfee: u64,
    pub(crate) difficulty: u64,
    pub(crate) staking_treasury: u64,
    avg_income: u64,
    avg_variance: u64,
    avg_atr_income: u64,
    avg_atr_variance: u64,
    /// Transactions
    pub transactions: Vec<Transaction>,
    /// Self-Calculated / Validated
    pub(crate) pre_hash: SaitoHash,
    /// Self-Calculated / Validated
    pub hash: SaitoHash,
    /// total fees paid into block
    total_fees: u64,
    /// total routing work in block, given creator
    total_work: u64,
    /// Is Block on longest chain
    pub(crate) in_longest_chain: bool,
    // has golden ticket
    pub has_golden_ticket: bool,
    // has issuance transaction
    pub has_issuance_transaction: bool,
    // issuance transaction index
    pub issuance_transaction_index: u64,
    // has fee transaction
    has_fee_transaction: bool,
    // golden ticket index
    golden_ticket_index: u64,
    // fee transaction index
    fee_transaction_index: u64,
    // number of rebroadcast slips
    total_rebroadcast_slips: u64,
    // number of rebroadcast txs
    total_rebroadcast_nolan: u64,
    // all ATR txs hashed together
    rebroadcast_hash: [u8; 32],
    // the state of the block w/ pruning etc
    pub(crate) block_type: BlockType,
    // vector of staker slips spent this block - used to prevent withdrawals and payouts same block
    #[serde(skip)]
    pub slips_spent_this_block: AHashMap<SaitoUTXOSetKey, u64>,
    #[serde(skip)]
    created_hashmap_of_slips_spent_this_block: bool,
    // the peer's connection ID who sent us this block
    #[serde(skip)]
    pub(crate) source_connection_id: Option<SaitoPublicKey>,
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
            avg_variance: 0,
            avg_atr_income: 0,
            avg_atr_variance: 0,
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
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.push(tx);
    }

    //
    // returns valid block
    //
    pub async fn create(
        transactions: &mut Vec<Transaction>,
        previous_block_hash: SaitoHash,
        wallet_lock: Arc<RwLock<Wallet>>,
        blockchain: &mut Blockchain,
        current_timestamp: u64,
    ) -> Block {
        debug!(
            "Block::create : previous block hash : {:?}",
            hex::encode(previous_block_hash)
        );

        let public_key;
        {
            trace!("waiting for the wallet read lock");
            let wallet = wallet_lock.read().await;
            trace!("acquired the wallet read lock");
            public_key = wallet.public_key;
        }
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

        let current_burnfee: u64 =
            BurnFee::return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
                previous_block_burnfee,
                current_timestamp,
                previous_block_timestamp,
            );

        block.id = previous_block_id + 1;
        block.previous_block_hash = previous_block_hash;
        block.burnfee = current_burnfee;
        block.timestamp = current_timestamp;
        block.difficulty = previous_block_difficulty;

        block.creator = public_key;

        //
        // in-memory swap copying txs in block from mempool
        //
        mem::swap(&mut block.transactions, transactions);

        //
        // update slips_spent_this_block so that we have a record of
        // how many times input slips are spent in this block. we will
        // use this later to ensure there are no duplicates. this include
        // during the fee transaction, so that we cannot pay a staker
        // that is also paid this block otherwise.
        //
        // this will not include the fee transaction or the ATR txs
        // because they have not been added to teh block yet, but they
        // permit us to avoid paying out StakerWithdrawal slips when we
        // generate the fee payment.
        //
        // note -- no need to have an exception for the FEE TX here as
        // we have not added it yet.
        //
        if !block.created_hashmap_of_slips_spent_this_block {
            for transaction in &block.transactions {
                for input in transaction.inputs.iter() {
                    block
                        .slips_spent_this_block
                        .entry(input.get_utxoset_key())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                }
                block.created_hashmap_of_slips_spent_this_block = true;
            }
        }

        //
        // contextual values
        //
        let mut cv: ConsensusValues = block.generate_consensus_values(&blockchain).await;

        //
        // ATR transactions
        //
        let rlen = cv.rebroadcasts.len();
        // TODO -- figure out if there is a more efficient solution
        // than iterating through the entire transaction set here.
        let _tx_hashes_generated = cv.rebroadcasts[0..rlen]
            .par_iter_mut()
            .all(|tx| tx.generate(public_key));
        if rlen > 0 {
            block.transactions.append(&mut cv.rebroadcasts);
        }

        //
        // fee transactions
        //
        // if a golden ticket is included in THIS block Saito uses the randomness
        // associated with that golden ticket to create a fair output for the
        // previous block.
        //
        if cv.fee_transaction.is_some() {
            //
            // creator signs fee transaction
            //
            let mut fee_tx = cv.fee_transaction.unwrap();
            let hash_for_signature: SaitoHash = hash(&fee_tx.serialize_for_signature());
            fee_tx.hash_for_signature = Some(hash_for_signature);
            {
                trace!("waiting for the wallet read lock");
                let wallet = wallet_lock.read().await;
                trace!("acquired the wallet read lock");
                fee_tx.sign(wallet.private_key);
            }
            //
            // and we add it to the block
            //
            block.add_transaction(fee_tx);
        }

        //
        // update slips_spent_this_block so that we have a record of
        // how many times input slips are spent in this block. we will
        // use this later to ensure there are no duplicates. this include
        // during the fee transaction, so that we cannot pay a staker
        // that is also paid this block otherwise.
        //
        for transaction in &block.transactions {
            if transaction.transaction_type != TransactionType::Fee {
                for input in transaction.inputs.iter() {
                    block
                        .slips_spent_this_block
                        .entry(input.get_utxoset_key())
                        .and_modify(|e| *e += 1)
                        .or_insert(1);
                }
            }
        }
        block.created_hashmap_of_slips_spent_this_block = true;

        //
        // set difficulty
        //
        block.difficulty = cv.expected_difficulty;

        //
        // set treasury
        //
        if cv.nolan_falling_off_chain != 0 {
            block.treasury = previous_block_treasury + cv.nolan_falling_off_chain;
        }

        //
        // set staking treasury
        //
        if cv.staking_treasury != 0 {
            let mut adjusted_staking_treasury = previous_block_staking_treasury;
            if cv.staking_treasury < 0 {
                let x = cv.staking_treasury * -1;
                if adjusted_staking_treasury > x as u64 {
                    adjusted_staking_treasury -= x as u64;
                } else {
                    adjusted_staking_treasury = 0;
                }
            } else {
                adjusted_staking_treasury += cv.staking_treasury as u64;
            }
            // info!(
            //     "adjusted staking treasury written into block {}",
            //     adjusted_staking_treasury
            // );
            block.staking_treasury = adjusted_staking_treasury;
        }

        //
        // generate merkle root
        //
        let block_merkle_root = block.generate_merkle_root();
        block.merkle_root = block_merkle_root;

        {
            trace!("waiting for the wallet read lock");
            let wallet = wallet_lock.read().await;
            trace!("acquired the wallet read lock");

            block.generate_pre_hash();
            block.sign(wallet.private_key);
        }

        //
        // hash includes pre-hash and sig, so update
        //
        block.generate_hash();

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
    pub fn deserialize_from_net(bytes: &Vec<u8>) -> Block {
        // TODO : return Option<Block> to support invalid buffers
        let transactions_len: u32 = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let id: u64 = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
        let timestamp: u64 = u64::from_be_bytes(bytes[12..20].try_into().unwrap());
        let previous_block_hash: SaitoHash = bytes[20..52].try_into().unwrap();
        let creator: SaitoPublicKey = bytes[52..85].try_into().unwrap();
        let merkle_root: SaitoHash = bytes[85..117].try_into().unwrap();
        let signature: SaitoSignature = bytes[117..181].try_into().unwrap();

        let treasury: u64 = u64::from_be_bytes(bytes[181..189].try_into().unwrap());
        let staking_treasury: u64 = u64::from_be_bytes(bytes[189..197].try_into().unwrap());

        let burnfee: u64 = u64::from_be_bytes(bytes[197..205].try_into().unwrap());
        let difficulty: u64 = u64::from_be_bytes(bytes[205..213].try_into().unwrap());

        let avg_income: u64 = u64::from_be_bytes(bytes[213..221].try_into().unwrap());
        let avg_variance: u64 = u64::from_be_bytes(bytes[221..229].try_into().unwrap());
        let avg_atr_income: u64 = u64::from_be_bytes(bytes[229..237].try_into().unwrap());
        let avg_atr_variance: u64 = u64::from_be_bytes(bytes[237..245].try_into().unwrap());

        let mut transactions = vec![];
        let mut start_of_transaction_data = BLOCK_HEADER_SIZE;
        for _n in 0..transactions_len {
            let inputs_len: u32 = u32::from_be_bytes(
                bytes[start_of_transaction_data..start_of_transaction_data + 4]
                    .try_into()
                    .unwrap(),
            );
            let outputs_len: u32 = u32::from_be_bytes(
                bytes[start_of_transaction_data + 4..start_of_transaction_data + 8]
                    .try_into()
                    .unwrap(),
            );
            let message_len: usize = u32::from_be_bytes(
                bytes[start_of_transaction_data + 8..start_of_transaction_data + 12]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let path_len: usize = u32::from_be_bytes(
                bytes[start_of_transaction_data + 12..start_of_transaction_data + 16]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let end_of_transaction_data = start_of_transaction_data
                + TRANSACTION_SIZE
                + ((inputs_len + outputs_len) as usize * SLIP_SIZE)
                + message_len
                + path_len as usize * HOP_SIZE;
            let transaction = Transaction::deserialize_from_net(
                bytes[start_of_transaction_data..end_of_transaction_data].to_vec(),
            );
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
        block.avg_variance = avg_variance;
        block.avg_atr_income = avg_atr_income;
        block.avg_atr_variance = avg_atr_variance;
        block.transactions = transactions.to_vec();
        if transactions_len == 0 {
            block.block_type = BlockType::Header;
        }

        block
    }
    //
    // downgrade block
    //
    pub async fn downgrade_block_to_block_type(&mut self, block_type: BlockType) -> bool {
        info!("BLOCK_ID {:?}", self.id);

        if self.block_type == block_type {
            return true;
        }

        //
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

        //
        // find winning nolan
        //
        let x = U256::from_big_endian(&random_number);
        //
        // fee calculation should be the same used in block when
        // generating the fee transaction.
        //
        let y = self.total_fees;

        //
        // if there are no fees, payout to burn address
        //
        if y == 0 {
            winner_pubkey = [0; 33];
            return winner_pubkey;
        }

        let z = U256::from_big_endian(&y.to_be_bytes());
        let (zy, _bolres) = x.overflowing_rem(z);
        let winning_nolan = zy.low_u64();
        // we may need function-timelock object if we need to recreate
        // an ATR transaction to pick the winning routing node.
        let winning_tx_placeholder: Transaction;
        let mut winning_tx: &Transaction;

        //
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

        //
        // if winner is atr, we take inside TX
        //
        if winning_tx.transaction_type == TransactionType::ATR {
            let tmptx = winning_tx.message.to_vec();
            winning_tx_placeholder = Transaction::deserialize_from_net(tmptx);
            winning_tx = &winning_tx_placeholder;
        }

        //
        // hash random number to pick routing node
        //
        winner_pubkey = winning_tx.get_winning_routing_node(hash(&random_number.to_vec()));

        winner_pubkey
    }

    //
    // generate ancillary data
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
        // if we are generating the metadata for a block, we use the
        // public_key of the block creator when we calculate the fees
        // and the routing work.
        //
        let creator_public_key = self.creator;

        // ensure hashes correct
        self.generate_pre_hash();
        self.generate_hash();

        let _transactions_pre_calculated = &self
            .transactions
            .par_iter_mut()
            .all(|tx| tx.generate(creator_public_key));

        // trace!(" ... block.prevalid - pst hash:  {:?}", create_timestamp());

        //
        // we need to calculate the cumulative figures AFTER the
        // original figures.
        //
        let mut cumulative_fees = 0;
        let mut cumulative_work = 0;

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
            cumulative_work = transaction.generate_cumulative_work(cumulative_work);

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
            if !self.created_hashmap_of_slips_spent_this_block {
                if transaction.transaction_type != TransactionType::Fee {
                    for input in transaction.inputs.iter() {
                        self.slips_spent_this_block
                            .entry(input.get_utxoset_key())
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
                    }
                    self.created_hashmap_of_slips_spent_this_block = true;
                }
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

                    for input in transaction.inputs.iter() {
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

        //
        // update block with total fees
        //
        self.total_fees = cumulative_fees;
        self.total_work = cumulative_work;

        // trace!(
        //     " ... block.pre_validation_done:  {:?}",
        //     create_timestamp(),
        //     // tracing_tracker.time_since_last();
        // );

        true
    }

    pub fn generate_hash(&mut self) -> SaitoHash {
        let hash_for_hash = hash(&self.serialize_for_hash());
        self.hash = hash_for_hash;
        hash_for_hash
    }

    pub fn generate_merkle_root(&self) -> SaitoHash {
        debug!("generating the merkle root 1");
        debug!(
            "generating the merkle root len: {}",
            self.transactions.len()
        );

        match MerkleTree::generate(&self.transactions) {
            None => [0u8; 32],
            Some(tree) => tree.get_root_hash(),
        }
    }

    //
    // generate dynamic consensus values
    //
    pub async fn generate_consensus_values(&self, blockchain: &Blockchain) -> ConsensusValues {
        debug!("generate consensus values");
        let mut cv = ConsensusValues::new();

        //
        // calculate total fees
        //
        let mut index: usize = 0;
        for transaction in &self.transactions {
            if !transaction.is_fee_transaction() {
                cv.total_fees += transaction.total_fees;
            } else {
                cv.ft_num += 1;
                cv.ft_index = Some(index);
            }
            if transaction.is_golden_ticket() {
                cv.gt_num += 1;
                cv.gt_index = Some(index);
            }
            if transaction.is_issuance_transaction() {
                cv.it_num += 1;
                cv.it_index = Some(index);
            }
            index += 1;
        }

        //
        // calculate automatic transaction rebroadcasts / ATR / atr
        //
        if self.id > GENESIS_PERIOD {
            let pruned_block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(self.id - 2);

            //
            // generate metadata should have prepared us with a pre-prune block
            // that contains all of the transactions and is ready to have its
            // ATR rebroadcasts calculated.
            //
            if let Some(pruned_block) = blockchain.blocks.get(&pruned_block_hash) {
                //
                // identify all unspent transactions
                //
                for transaction in &pruned_block.transactions {
                    for output in transaction.outputs.iter() {
                        //
                        // these need to be calculated dynamically based on the
                        // value of the UTXO and the byte-size of the transaction
                        //
                        let REBROADCAST_FEE = 200_000_000;
                        let STAKING_SUBSIDY = 100_000_000;
                        let UTXO_ADJUSTMENT = REBROADCAST_FEE - STAKING_SUBSIDY;

                        //
                        // valid means spendable and non-zero
                        //HACK
                        if output.validate(&blockchain.utxoset) {
                            if output.amount > UTXO_ADJUSTMENT {
                                cv.total_rebroadcast_nolan += output.amount;
                                cv.total_rebroadcast_fees_nolan += REBROADCAST_FEE;
                                cv.total_rebroadcast_staking_payouts_nolan += STAKING_SUBSIDY;
                                cv.total_rebroadcast_slips += 1;

                                //
                                // create rebroadcast transaction
                                //
                                let rebroadcast_transaction =
                                    Transaction::create_rebroadcast_transaction(
                                        &transaction,
                                        output,
                                        REBROADCAST_FEE,
                                        STAKING_SUBSIDY,
                                    );

                                //
                                // update cryptographic hash of all ATRs
                                //
                                let mut vbytes: Vec<u8> = vec![];
                                vbytes.extend(&cv.rebroadcast_hash);
                                vbytes.extend(&rebroadcast_transaction.serialize_for_signature());
                                cv.rebroadcast_hash = hash(&vbytes);

                                cv.rebroadcasts.push(rebroadcast_transaction);
                            } else {
                                //
                                // rebroadcast dust is either collected into the treasury or
                                // distributed as a fee for the next block producer. for now
                                // we will simply distribute it as a fee. we may need to
                                // change this if the DUST becomes a significant enough amount
                                // each block to reduce consensus security.
                                //
                                cv.total_rebroadcast_fees_nolan += output.amount;
                            }
                        }
                    }
                }
            }
        }

        //
        // burn fee, difficulty and avg_income figures
        //
        if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
            cv.avg_income = previous_block.avg_income;
            cv.avg_variance = previous_block.avg_variance;
            cv.avg_atr_income = previous_block.avg_atr_income;
            cv.avg_atr_variance = previous_block.avg_atr_variance;

            if previous_block.avg_income > cv.total_fees {
                let adjustment = (previous_block.avg_income - cv.total_fees) / GENESIS_PERIOD;
                if adjustment > 0 {
                    cv.avg_income -= adjustment;
                }
            }
            if previous_block.avg_income < cv.total_fees {
                let adjustment = (cv.total_fees - previous_block.avg_income) / GENESIS_PERIOD;
                if adjustment > 0 {
                    cv.avg_income += adjustment;
                }
            }

            //
            // average atr income and variance adjusts slowly.
            //
            if previous_block.avg_atr_income > cv.total_rebroadcast_nolan {
                let adjustment =
                    (previous_block.avg_atr_income - cv.total_rebroadcast_nolan) / GENESIS_PERIOD;
                if adjustment > 0 {
                    cv.avg_atr_income -= adjustment;
                }
            }
            if previous_block.avg_atr_income < cv.total_rebroadcast_nolan {
                let adjustment =
                    (cv.total_rebroadcast_nolan - previous_block.avg_atr_income) / GENESIS_PERIOD;
                if adjustment > 0 {
                    cv.avg_atr_income += adjustment;
                }
            }

            let difficulty = previous_block.difficulty;
            if !previous_block.has_golden_ticket && cv.gt_num == 0 {
                if difficulty > 0 {
                    cv.expected_difficulty = previous_block.difficulty - 1;
                }
            } else if previous_block.has_golden_ticket && cv.gt_num > 0 {
                cv.expected_difficulty = difficulty + 1;
            } else {
                cv.expected_difficulty = difficulty;
            }
        } else {
            //
            // if there is no previous block, the burn fee is not adjusted. validation
            // rules will cause the block to fail unless it is the first block. average
            // income is set to whatever the block avg_income is set to.
            //
            cv.avg_income = self.avg_income;
            cv.avg_variance = self.avg_variance;
            cv.avg_atr_income = self.avg_atr_income;
            cv.avg_atr_variance = self.avg_atr_variance;
        }

        //
        // calculate payments to miners / routers / stakers
        //
        if let Some(gt_index) = cv.gt_index {
            let golden_ticket: GoldenTicket =
                GoldenTicket::deserialize_from_net(self.transactions[gt_index].message.to_vec());
            // generate input hash for router
            let mut next_random_number = hash(&golden_ticket.random.to_vec());
            let _miner_public_key = golden_ticket.public_key;

            //
            // miner payout is fees from previous block, no staking treasury
            //
            if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
                //
                // limit previous block payout to avg income
                //
                let mut previous_block_payout = previous_block.total_fees;
                if previous_block_payout > (previous_block.avg_income as f64 * 1.25) as u64
                    && previous_block_payout > 50
                {
                    previous_block_payout = (previous_block.avg_income as f64 * 1.24) as u64;
                }

                let miner_payment = previous_block_payout / 2;
                let router_payment = previous_block_payout - miner_payment;

                //
                // calculate miner and router payments
                //
                let router_public_key = previous_block.find_winning_router(next_random_number);

                let mut payout = BlockPayout::new();
                payout.miner = golden_ticket.public_key;
                payout.router = router_public_key;
                payout.miner_payout = miner_payment;
                payout.router_payout = router_payment;
                cv.block_payout.push(payout);

                //
                // these two from find_winning_router - 3, 4
                //
                next_random_number = hash(&next_random_number.to_vec());
                next_random_number = hash(&next_random_number.to_vec());

                //
                // loop backwards until MAX recursion OR golden ticket
                //
                let mut cont = 1;
                let mut loop_index = 0;
                let mut did_the_block_before_our_staking_block_have_a_golden_ticket =
                    previous_block.has_golden_ticket;
                //
                // staking block hash is 3 back, pre
                //
                let mut staking_block_hash = previous_block.previous_block_hash;

                while cont == 1 {
                    loop_index += 1;

                    //
                    // we start with the second block, so once loop_IDX hits the same
                    // number as MAX_STAKER_RECURSION we have processed N blocks where
                    // N is MAX_STAKER_RECURSION.
                    //
                    if loop_index >= MAX_STAKER_RECURSION {
                        cont = 0;
                    } else {
                        if let Some(staking_block) = blockchain.blocks.get(&staking_block_hash) {
                            staking_block_hash = staking_block.previous_block_hash;
                            if !did_the_block_before_our_staking_block_have_a_golden_ticket {
                                //
                                // update with this block info in case of next loop
                                //
                                did_the_block_before_our_staking_block_have_a_golden_ticket =
                                    staking_block.has_golden_ticket;

                                //
                                // calculate staker and router payments
                                //
                                // the staker payout is contained in the slip of the winner. this is
                                // because we calculate it afresh every time we reset the staking table
                                // the payment for the router requires calculating the amount that will
                                // be withheld for the staker treasury, which is what previous_staker_
                                // payment is measuring.
                                //
                                let mut previous_staking_block_payout = staking_block.total_fees;
                                if previous_staking_block_payout
                                    > (staking_block.avg_income as f64 * 1.25) as u64
                                    && previous_staking_block_payout > 50
                                {
                                    previous_staking_block_payout =
                                        (staking_block.avg_income as f64 * 1.24) as u64;
                                }

                                let sp = previous_staking_block_payout / 2;
                                let rp = previous_staking_block_payout - sp;

                                let mut payout = BlockPayout::new();
                                payout.router =
                                    staking_block.find_winning_router(next_random_number);
                                payout.router_payout = rp;
                                payout.staking_treasury = sp as i64;

                                // router consumes 2 hashes
                                next_random_number = hash(&next_random_number.to_vec());
                                next_random_number = hash(&next_random_number.to_vec());

                                cv.block_payout.push(payout);
                            }
                        }
                    }
                }
            }

            //
            // now create fee transaction using the block payout data
            //
            let mut slip_index = 0;
            let mut transaction = Transaction::new();
            transaction.transaction_type = TransactionType::Fee;

            for i in 0..cv.block_payout.len() {
                if cv.block_payout[i].miner != [0; 33] {
                    let mut output = Slip::new();
                    output.public_key = cv.block_payout[i].miner;
                    output.amount = cv.block_payout[i].miner_payout;
                    output.slip_type = SlipType::MinerOutput;
                    output.slip_index = slip_index;
                    transaction.add_output(output.clone());
                    slip_index += 1;
                }
                if cv.block_payout[i].router != [0; 33] {
                    let mut output = Slip::new();
                    output.public_key = cv.block_payout[i].router;
                    output.amount = cv.block_payout[i].router_payout;
                    output.slip_type = SlipType::RouterOutput;
                    output.slip_index = slip_index;
                    transaction.add_output(output.clone());
                    slip_index += 1;
                }
            }

            cv.fee_transaction = Some(transaction);
        }

        //
        // if there is no golden ticket AND there is no golden ticket before the MAX
        // blocks we recurse to collect NOLAN we have to add the amount of the unpaid
        // block to the amount of NOLAN that is falling off our chain.
        //
        // this edge-case should be a statistical abnormality that we almost never
        // run into, but it is good to collect the SAITO into a variable that we track
        // so that we can confirm the soundness of monetary policy by monitoring the
        // blockchain.
        //
        if cv.gt_num == 0 {
            for i in 1..=MAX_STAKER_RECURSION {
                if i >= self.id {
                    break;
                }

                let bid = self.id - i;
                let previous_block_hash = blockchain
                    .blockring
                    .get_longest_chain_block_hash_by_block_id(bid);

                // previous block hash can be [0; 32] if there is no longest-chain block

                if previous_block_hash != [0; 32] {
                    let previous_block = blockchain.get_block(&previous_block_hash).await.unwrap();

                    if previous_block.has_golden_ticket {
                        break;
                    } else {
                        //
                        // this is the block BEFORE from which we need to collect the nolan due to
                        // our iterator starting at 0 for the current block. i.e. if MAX_STAKER_
                        // RECURSION is 3, at 3 we are the fourth block back.
                        //
                        if i == MAX_STAKER_RECURSION {
                            cv.nolan_falling_off_chain = previous_block.total_fees;
                        }
                    }
                }
            }
        }

        cv
    }
    pub fn generate_pre_hash(&mut self) {
        let hash_for_signature = hash(&self.serialize_for_signature());
        self.pre_hash = hash_for_signature;
    }

    pub fn on_chain_reorganization(&self, utxoset: &mut UtxoSet, longest_chain: bool) -> bool {
        for tx in &self.transactions {
            tx.on_chain_reorganization(utxoset, longest_chain, self.id);
        }
        true
    }

    //
    // we may want to separate the signing of the block from the setting of the necessary hash
    // we do this together out of convenience only
    //
    pub fn sign(&mut self, private_key: SaitoPrivateKey) {
        //
        // we set final data
        //
        self.signature = sign(&self.pre_hash, private_key);
    }

    // serialize the pre_hash and the signature_for_source into a
    // bytes array that can be hashed and then have the hash set.
    pub fn serialize_for_hash(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.pre_hash);
        vbytes.extend(&self.signature);
        vbytes.extend(&self.previous_block_hash);
        vbytes
    }

    // serialize major block components for block signature
    // this will manually calculate the merkle_root if necessary
    // but it is advised that the merkle_root be already calculated
    // to avoid speed issues.
    pub fn serialize_for_signature(&self) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.id.to_be_bytes());
        vbytes.extend(&self.timestamp.to_be_bytes());
        vbytes.extend(&self.previous_block_hash);
        vbytes.extend(&self.creator);
        vbytes.extend(&self.merkle_root);
        vbytes.extend(&self.treasury.to_be_bytes());
        vbytes.extend(&self.staking_treasury.to_be_bytes());
        vbytes.extend(&self.burnfee.to_be_bytes());
        vbytes.extend(&self.difficulty.to_be_bytes());
        vbytes.extend(&self.avg_income.to_be_bytes());
        vbytes.extend(&self.avg_variance.to_be_bytes());
        vbytes.extend(&self.avg_atr_income.to_be_bytes());
        vbytes.extend(&self.avg_atr_variance.to_be_bytes());
        vbytes
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
    /// [avg_variance - 8 bytes - u64]
    /// [avg_atr_income - 8 bytes - u64]
    /// [avg_atr_variance - 8 bytes - u64]
    /// [transaction][transaction][transaction]...
    pub fn serialize_for_net(&self, block_type: BlockType) -> Vec<u8> {
        let mut vbytes: Vec<u8> = vec![];

        // block headers do not get tx data
        if block_type == BlockType::Header {
            vbytes.extend(&(0 as u32).to_be_bytes());
        } else {
            vbytes.extend(&(self.transactions.iter().len() as u32).to_be_bytes());
        }

        vbytes.extend(&self.id.to_be_bytes());
        vbytes.extend(&self.timestamp.to_be_bytes());
        vbytes.extend(&self.previous_block_hash);
        vbytes.extend(&self.creator);
        vbytes.extend(&self.merkle_root);
        vbytes.extend(&self.signature);
        vbytes.extend(&self.treasury.to_be_bytes());
        vbytes.extend(&self.staking_treasury.to_be_bytes());
        vbytes.extend(&self.burnfee.to_be_bytes());
        vbytes.extend(&self.difficulty.to_be_bytes());
        vbytes.extend(&self.avg_income.to_be_bytes());
        vbytes.extend(&self.avg_variance.to_be_bytes());
        vbytes.extend(&self.avg_atr_income.to_be_bytes());
        vbytes.extend(&self.avg_atr_variance.to_be_bytes());

        let mut serialized_txs = vec![];

        // block headers do not get tx data
        if block_type != BlockType::Header {
            self.transactions.iter().for_each(|transaction| {
                serialized_txs.extend(transaction.serialize_for_net());
            });
            vbytes.extend(serialized_txs);
        }

        vbytes
    }

    pub async fn update_block_to_block_type(
        &mut self,
        block_type: BlockType,
        storage: &Storage,
    ) -> bool {
        if self.block_type == block_type {
            return true;
        }

        if block_type == BlockType::Full {
            return self.upgrade_block_to_block_type(block_type, storage).await;
        }

        if block_type == BlockType::Pruned {
            return self.downgrade_block_to_block_type(block_type).await;
        }

        return false;
    }

    //
    // if the block is not at the proper type, try to upgrade it to have the
    // data that is necessary for blocks of that type if possible. if this is
    // not possible, return false. if it is possible, return true once upgraded.
    //
    pub async fn upgrade_block_to_block_type(
        &mut self,
        block_type: BlockType,
        storage: &Storage,
    ) -> bool {
        trace!("UPGRADE_BLOCK_TO_BLOCK_TYPE {:?}", self.block_type);
        if self.block_type == block_type {
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
            let mut new_block = storage
                .load_block_from_disk(storage.generate_block_filename(&self))
                .await
                .unwrap();
            let hash_for_signature = hash(&new_block.serialize_for_signature());
            new_block.pre_hash = hash_for_signature;
            let hash_for_hash = hash(&new_block.serialize_for_hash());
            new_block.hash = hash_for_hash;

            //
            // in-memory swap copying txs in block from mempool
            //
            mem::swap(&mut new_block.transactions, &mut self.transactions);
            //
            // transactions need hashes
            //
            self.generate();
            self.block_type = BlockType::Full;

            return true;
        }

        false
    }

    pub async fn validate(&self, blockchain: &Blockchain, utxoset: &UtxoSet) -> bool {
        //
        // no transactions? no thank you
        //
        if self.transactions.is_empty() {
            error!("ERROR 424342: block does not validate as it has no transactions",);
            return false;
        }

        //
        // trace!(
        //     " ... block.validate: (burn fee)  {:?}",
        //     create_timestamp(),
        //     // tracing_tracker.time_since_last();
        // );

        // verify signed by creator
        if !verify(
            &self.serialize_for_signature(),
            self.signature,
            self.creator,
        ) {
            error!("ERROR 582039: block is not signed by creator or signature does not validate",);
            return false;
        }

        //
        // Consensus Values
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
        let cv = self.generate_consensus_values(&blockchain).await;

        //
        // only block #1 can have an issuance transaction
        //
        if cv.it_num > 0 && self.id > 1 {
            error!("ERROR: blockchain contains issuance after block 1 in chain",);
            return false;
        }

        //
        // Previous Block
        //
        // many kinds of validation like the burn fee and the golden ticket solution
        // require the existence of the previous block in order to validate. we put all
        // of these validation steps below so they will have access to the previous block
        //
        // if no previous block exists, we are valid only in a limited number of
        // circumstances, such as this being the first block we are adding to our chain.
        //
        if let Some(previous_block) = blockchain.blocks.get(&self.previous_block_hash) {
            //
            // validate treasury
            //
            if self.treasury != previous_block.treasury + cv.nolan_falling_off_chain {
                error!(
                    "ERROR: treasury does not validate: {} expected versus {} found",
                    (previous_block.treasury + cv.nolan_falling_off_chain),
                    self.treasury,
                    // tracing_tracker.time_since_last();
                );
                return false;
            }

            //
            // validate staking treasury
            //
            let mut adjusted_staking_treasury = previous_block.staking_treasury;
            if cv.staking_treasury < 0 {
                let x = cv.staking_treasury * -1;
                if adjusted_staking_treasury > x as u64 {
                    adjusted_staking_treasury -= x as u64;
                } else {
                    adjusted_staking_treasury = 0;
                }
            } else {
                let x: u64 = cv.staking_treasury as u64;
                adjusted_staking_treasury += x;
            }

            if self.staking_treasury != adjusted_staking_treasury {
                error!(
                    "ERROR: staking treasury does not validate: {} expected versus {} found",
                    adjusted_staking_treasury, self.staking_treasury,
                );
                //     "ERROR: staking treasury does not validate: {} expected versus {} found",
                //     adjusted_staking_treasury,
                //     self.get_staking_treasury(),
                return false;
            }

            //
            // validate burn fee
            //
            let new_burnfee: u64 =
                BurnFee::return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
                    previous_block.burnfee,
                    self.timestamp,
                    previous_block.timestamp,
                );
            if new_burnfee != self.burnfee {
                error!(
                    "ERROR: burn fee does not validate, expected: {}",
                    new_burnfee
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
            let amount_of_routing_work_needed: u64 =
                BurnFee::return_routing_work_needed_to_produce_block_in_nolan(
                    previous_block.burnfee,
                    self.timestamp,
                    previous_block.timestamp,
                );
            if self.total_work < amount_of_routing_work_needed {
                error!("Error 510293: block lacking adequate routing work from creator");
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
                let golden_ticket: GoldenTicket = GoldenTicket::deserialize_from_net(
                    self.transactions[gt_index].message.to_vec(),
                );
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
                        "ERROR: Golden Ticket solution does not validate against previous block hash and difficulty"
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
        if self.merkle_root == [0; 32] && self.merkle_root != self.generate_merkle_root() {
            error!("merkle root is unset or is invalid false 1");
            return false;
        }

        // trace!(" ... block.validate: (cv-data)   {:?}", create_timestamp());

        //
        // validate fee transactions
        //
        // if this block contains a golden ticket, we have to use the random
        // number associated with the golden ticket to create a fee-transaction
        // that stretches back into previous blocks and finds the winning nodes
        // that should collect payment.
        //
        if let (Some(ft_index), Some(mut fee_transaction)) = (cv.ft_index, cv.fee_transaction) {
            //
            // no golden ticket? invalid
            //
            if cv.gt_index.is_none() {
                error!("ERROR 48203: block appears to have fee transaction without golden ticket");
                return false;
            }

            //
            // the fee transaction we receive from the CV needs to be updated with
            // block-specific data in the same way that all of the transactions in
            // the block have been. we must do this prior to comparing them.
            //
            fee_transaction.generate(self.creator);

            let hash1 = hash(&fee_transaction.serialize_for_signature());
            let hash2 = hash(&self.transactions[ft_index].serialize_for_signature());
            if hash1 != hash2 {
                error!(
                    "ERROR 627428: block {} fee transaction doesn't match cv fee transaction",
                    self.id
                );
                info!("fee transaction = {:?}", fee_transaction);
                info!("tx : {:?}", self.transactions[ft_index]);
                return false;
            }
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
                "difficulty is false {} vs {}",
                cv.expected_difficulty, self.difficulty
            );
            return false;
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
        // TODO - remove when convenient. when transactions fail to validate using
        // parallel processing can make it difficult to find out exactly what the
        // problem is. ergo this code that tries to do them on the main thread so
        // debugging output works.
        //
        for i in 0..self.transactions.len() {
            let transactions_valid2 = self.transactions[i].validate(utxoset);
            if !transactions_valid2 {
                info!("Type: {:?}", self.transactions[i].transaction_type);
                // info!("Data {:?}", self.transactions[i]);
            }
        }
        //true

        let transactions_valid = self.transactions.par_iter().all(|tx| tx.validate(utxoset));

        transactions_valid
    }
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;
    use futures::future::join_all;
    use hex::FromHex;

    use crate::common::test_manager::test::TestManager;
    use crate::core::data::block::{Block, BlockType};
    use crate::core::data::crypto::verify;
    use crate::core::data::slip::Slip;
    use crate::core::data::transaction::{Transaction, TransactionType};
    use crate::core::data::wallet::Wallet;

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
        block.timestamp = 1637034582666;
        block.previous_block_hash = <[u8; 32]>::from_hex(
            "bcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b",
        )
        .unwrap();
        block.merkle_root = <[u8; 32]>::from_hex(
            "ccf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8b",
        )
        .unwrap();
        block.creator = <[u8; 33]>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();
        block.burnfee = 50000000;
        block.difficulty = 0;
        block.treasury = 0;
        block.staking_treasury = 0;
        block.signature = <[u8; 64]>::from_hex("c9a6c2d0bf884be6933878577171a3c8094c2bf6e0bc1b4ec3535a4a55224d186d4d891e254736cae6c0d2002c8dfc0ddfc7fcdbe4bc583f96fa5b273b9d63f4").unwrap();

        let serialized_body = block.serialize_for_signature();
        assert_eq!(serialized_body.len(), 177);

        block.creator = <[u8; 33]>::from_hex(
            "dcf6cceb74717f98c3f7239459bb36fdcd8f350eedbfccfbebf7c0b0161fcd8bcc",
        )
        .unwrap();

        block.sign(
            <[u8; 32]>::from_hex(
                "854702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235d",
            )
            .unwrap(),
        );

        assert_eq!(block.signature.len(), 64);
        assert_eq!(
            block.signature,
            [
                79, 111, 17, 122, 189, 142, 78, 252, 111, 231, 122, 86, 129, 151, 99, 71, 245, 34,
                33, 254, 104, 138, 238, 136, 230, 45, 113, 171, 146, 105, 138, 64, 43, 25, 204,
                186, 169, 208, 222, 5, 89, 64, 83, 32, 102, 18, 114, 20, 171, 0, 97, 232, 158, 108,
                185, 37, 225, 233, 33, 97, 222, 132, 218, 120
            ]
        )
    }

    #[test]
    fn block_serialization_and_deserialization_test() {
        let mock_input = Slip::new();
        let mock_output = Slip::new();

        let mut mock_tx = Transaction::new();
        mock_tx.timestamp = 0;
        mock_tx.add_input(mock_input.clone());
        mock_tx.add_output(mock_output.clone());
        mock_tx.message = vec![104, 101, 108, 111];
        mock_tx.transaction_type = TransactionType::Normal;
        mock_tx.signature = [1; 64];

        let mut mock_tx2 = Transaction::new();
        mock_tx2.timestamp = 0;
        mock_tx2.add_input(mock_input);
        mock_tx2.add_output(mock_output);
        mock_tx2.message = vec![];
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
        block.treasury = 1;
        block.burnfee = 2;
        block.difficulty = 3;
        block.transactions = vec![mock_tx, mock_tx2];

        let serialized_block = block.serialize_for_net(BlockType::Full);
        let deserialized_block = Block::deserialize_from_net(&serialized_block);

        let serialized_block_header = block.serialize_for_net(BlockType::Header);
        let deserialized_block_header = Block::deserialize_from_net(&serialized_block_header);

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
        assert_eq!(deserialized_block.treasury, 1);
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
        assert_eq!(deserialized_block_header.treasury, 1);
        assert_eq!(deserialized_block_header.burnfee, 2);
        assert_eq!(deserialized_block_header.difficulty, 3);
    }

    #[test]
    fn block_sign_and_verify_test() {
        let wallet = Wallet::new();
        let mut block = Block::new();
        block.creator = wallet.public_key;
        block.generate();
        block.sign(wallet.private_key);
        block.generate_hash();

        assert_eq!(block.creator, wallet.public_key);
        assert_eq!(
            verify(&block.pre_hash, block.signature, block.creator,),
            true
        );
        assert_ne!(block.hash, [0; 32]);
        assert_ne!(block.signature, [0; 64]);
    }

    #[test]
    fn block_merkle_root_test() {
        let mut block = Block::new();
        let wallet = Wallet::new();

        let transactions: Vec<Transaction> = (0..5)
            .into_iter()
            .map(|_| {
                let mut transaction = Transaction::new();
                transaction.sign(wallet.private_key);
                transaction
            })
            .collect();

        block.transactions = transactions;
        block.merkle_root = block.generate_merkle_root();

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
            let mut transaction = Transaction::new();
            let wallet = wallet_lock.read().await;
            transaction.sign(wallet.private_key);
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
            .update_block_to_block_type(BlockType::Pruned, &mut t.storage)
            .await;

        assert_eq!(block.transactions.len(), 0);
        assert_eq!(block.block_type, BlockType::Pruned);

        block
            .update_block_to_block_type(BlockType::Full, &mut t.storage)
            .await;

        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.block_type, BlockType::Full);
        assert_eq!(
            serialized_full_block,
            block.serialize_for_net(BlockType::Full)
        );
    }
}

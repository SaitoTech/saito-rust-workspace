use std::collections::VecDeque;
use std::fmt::Debug;
use std::io::Error;
use std::ops::Deref;
use std::sync::Arc;

use ahash::{AHashMap, HashMap, HashSet};
use async_recursion::async_recursion;
use log::{debug, error, info, trace, warn};
use rayon::prelude::*;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

use crate::common::defs::{
    push_lock, Currency, PrintForLog, SaitoHash, SaitoPublicKey, Timestamp, UtxoSet,
    GENESIS_PERIOD, LOCK_ORDER_MEMPOOL, LOCK_ORDER_WALLET, MAX_STAKER_RECURSION,
    MIN_GOLDEN_TICKETS_DENOMINATOR, MIN_GOLDEN_TICKETS_NUMERATOR, PRUNE_AFTER_BLOCKS,
};
use crate::common::interface_io::InterfaceEvent;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockring::BlockRing;
use crate::core::data::configuration::Configuration;
use crate::core::data::mempool::Mempool;
use crate::core::data::network::Network;
use crate::core::data::slip::Slip;
use crate::core::data::storage::Storage;
use crate::core::data::transaction::{Transaction, TransactionType};
use crate::core::data::wallet::Wallet;
use crate::core::mining_thread::MiningEvent;
use crate::core::util::balance_snapshot::BalanceSnapshot;
use crate::{drain, iterate, lock_for_read, lock_for_write};

pub fn bit_pack(top: u32, bottom: u32) -> u64 {
    ((top as u64) << 32) + (bottom as u64)
}

pub fn bit_unpack(packed: u64) -> (u32, u32) {
    // Casting from a larger integer to a smaller integer (e.g. u32 -> u8) will truncate, no need to mask this
    let bottom = packed as u32;
    let top = (packed >> 32) as u32;
    (top, bottom)
}

#[derive(Debug)]
pub enum AddBlockResult {
    BlockAdded,
    BlockAlreadyExists,
    FailedButRetry,
    FailedNotValid,
}

#[derive(Debug)]
pub struct Blockchain {
    pub utxoset: UtxoSet,
    pub blockring: BlockRing,
    pub blocks: AHashMap<SaitoHash, Block>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    pub genesis_block_id: u64,
    pub fork_id: SaitoHash,
    pub last_block_hash: SaitoHash,
    pub last_block_id: u64,
    pub last_timestamp: u64,
    pub last_burnfee: Currency,

    pub genesis_timestamp: u64,
    genesis_block_hash: SaitoHash,
    pub lowest_acceptable_timestamp: u64,
    pub lowest_acceptable_block_hash: SaitoHash,
    pub lowest_acceptable_block_id: u64,

    blocks_fetching: HashSet<SaitoHash>,
}

impl Blockchain {
    #[allow(clippy::new_without_default)]
    pub fn new(wallet_lock: Arc<RwLock<Wallet>>) -> Self {
        Blockchain {
            utxoset: AHashMap::new(),
            blockring: BlockRing::new(),
            blocks: AHashMap::new(),
            wallet_lock,
            genesis_block_id: 0,
            fork_id: [0; 32],
            last_block_hash: [0; 32],
            last_block_id: 0,
            last_timestamp: 0,
            last_burnfee: 0,
            genesis_timestamp: 0,
            genesis_block_hash: [0; 32],
            lowest_acceptable_timestamp: 0,
            lowest_acceptable_block_hash: [0; 32],
            lowest_acceptable_block_id: 0,
            blocks_fetching: Default::default(),
        }
    }
    pub fn init(&mut self) -> Result<(), Error> {
        Ok(())
    }

    pub fn set_fork_id(&mut self, fork_id: SaitoHash) {
        debug!("setting fork id as : {:?}", fork_id.to_hex());
        self.fork_id = fork_id;
    }

    pub fn get_fork_id(&self) -> &SaitoHash {
        &self.fork_id
    }

    // #[async_recursion]
    pub async fn add_block(
        &mut self,
        mut block: Block,
        network: Option<&Network>,
        storage: &mut Storage,
        sender_to_miner: Option<Sender<MiningEvent>>,
        mempool: &mut Mempool,
        configs: &(dyn Configuration + Send + Sync),
    ) -> AddBlockResult {
        // confirm hash first
        // block.generate_pre_hash();
        // block.generate_hash();
        block.generate();

        let non_spv_txs = block
            .transactions
            .iter()
            .filter(|tx| tx.transaction_type != TransactionType::SPV)
            .count();
        debug!(
            "add_block {:?} of type : {:?} with id : {:?} with latest id : {:?} with tx count : {:?}/{:?}",
            block.hash.to_hex(),
            block.block_type,
            block.id,
            self.get_latest_block_id(),
            non_spv_txs,
            block.transactions.len()
        );

        // since we already have the block we unset the fetching state
        self.unmark_as_fetching(&block.hash);

        // trace!("block : {:?}", block);

        // start by extracting some variables that we will use
        // repeatedly in the course of adding this block to the
        // blockchain and our various indices.
        let block_hash = block.hash;
        let block_id = block.id;
        let previous_block_hash = self.blockring.get_latest_block_hash();
        // let previous_block_hash = block.previous_block_hash;

        // sanity checks
        if self.blocks.contains_key(&block_hash) {
            error!(
                "block already exists in blockchain {:?}. not adding",
                block.hash.to_hex()
            );
            return AddBlockResult::BlockAlreadyExists;
        }

        // TODO -- david review -- should be no need for recursive fetch
        // as each block will fetch the parent on arrival and processing
        // and we may want to tag and use the degree of distance to impose
        // penalties on routing peers.
        //
        // get missing block
        //
        if !self.blockring.is_empty() && self.get_block(&block.previous_block_hash).is_none() {
            if block.previous_block_hash == [0; 32] {
                info!(
                    "hash is empty for parent of block : {:?}",
                    block.hash.to_hex()
                );
            } else if block.source_connection_id.is_some() {
                let block_hash = block.previous_block_hash;

                let previous_is_in_queue;
                {
                    previous_is_in_queue =
                        iterate!(mempool.blocks_queue, 100).any(|b| block_hash == b.hash);
                }
                if !previous_is_in_queue {
                    if network.is_some() && !self.is_block_fetching(&block_hash) {
                        info!(
                            "fetching missing block : {:?} before adding block : {:?}",
                            block_hash.to_hex(),
                            block.hash.to_hex()
                        );
                        self.mark_as_fetching(block_hash);
                        let result = network
                            .unwrap()
                            .fetch_missing_block(
                                block_hash,
                                block.source_connection_id.as_ref().unwrap(),
                            )
                            .await;
                        if result.is_err() {
                            warn!(
                                "couldn't fetch parent block : {:?} for block : {:?}. unmarking the block",
                                block.previous_block_hash.to_hex(),
                                block.hash.to_hex()
                            );
                            self.unmark_as_fetching(&block_hash);
                        }
                    }
                } else {
                    debug!(
                        "previous block : {:?} is in the mempool. not fetching",
                        block_hash.to_hex()
                    );
                }

                debug!("adding block : {:?} back to mempool so it can be processed again after the previous block : {:?} is added",
                                    block.hash.to_hex(),
                                    block.previous_block_hash.to_hex());
                // TODO : mempool can grow if an attacker keep sending blocks with non existing parents. need to fix. can use an expiry time perhaps?
                mempool.add_block(block);
                return AddBlockResult::FailedButRetry;
            } else {
                debug!(
                    "block : {:?} source connection id not set",
                    block.hash.to_hex()
                );
            }
        } else {
            debug!(
                "previous block : {:?} exists in blockchain",
                block.previous_block_hash.to_hex()
            );
        }

        // pre-validation
        //
        // this would be a great place to put in a prevalidation check
        // once we are finished implementing Saito Classic. Goal would
        // be a fast form of lite-validation just to determine that it
        // is worth going through the more general effort of evaluating
        // this block for consensus.
        //

        // save block to disk
        //
        // we have traditionally saved blocks to disk AFTER validating them
        // but this can slow down block propagation. So it may be sensible
        // to start a save earlier-on in the process so that we can relay
        // the block faster serving it off-disk instead of fetching it
        // repeatedly from memory. Exactly when to do this is left as an
        // optimization exercise.
        //

        // insert block into hashmap and index
        //
        // the blockring is a BlockRing which lets us know which blocks (at which depth)
        // form part of the longest-chain. We also use the BlockRing to track information
        // on network congestion (how many block candidates exist at various depths and
        // in the future potentially the amount of work on each viable fork chain.
        //
        // we are going to transfer ownership of the block into the HashMap that stores
        // the block next, so we insert it into our BlockRing first as that will avoid
        // needing to borrow the value back for insertion into the BlockRing.
        //
        // TODO : check if this "if" condition can be moved to an assert
        if !self
            .blockring
            .contains_block_hash_at_block_id(block_id, block_hash)
        {
            self.blockring.add_block(&block);
        } else {
            // error!(
            //     "block : {:?} is already in blockring. therefore not adding",
            //     block.hash..to_hex()
            // );
            // return AddBlockResult::BlockAlreadyExists;
        }
        // blocks are stored in a hashmap indexed by the block_hash. we expect all
        // all block_hashes to be unique, so simply insert blocks one-by-one on
        // arrival if they do not exist.

        if !self.blocks.contains_key(&block_hash) {
            self.blocks.insert(block_hash, block);
        } else {
            error!(
                "BLOCK IS ALREADY IN THE BLOCKCHAIN, WHY ARE WE ADDING IT????? {:?}",
                block.hash.to_hex()
            );
            return AddBlockResult::BlockAlreadyExists;
        }

        // find shared ancestor of new_block with old_chain
        //
        let mut new_chain: Vec<[u8; 32]> = Vec::new();
        let mut old_chain: Vec<[u8; 32]> = Vec::new();
        let mut shared_ancestor_found = false;
        let mut new_chain_hash = block_hash;
        let mut old_chain_hash = previous_block_hash;
        let mut am_i_the_longest_chain = false;

        while !shared_ancestor_found {
            trace!("checking new chain hash : {:?}", new_chain_hash.to_hex());
            if let Some(block) = self.blocks.get(&new_chain_hash) {
                if block.in_longest_chain {
                    shared_ancestor_found = true;
                    trace!("shared ancestor found : {:?}", new_chain_hash.to_hex());
                    break;
                } else if new_chain_hash == [0; 32] {
                    break;
                }
                new_chain.push(new_chain_hash);
                new_chain_hash = self
                    .blocks
                    .get(&new_chain_hash)
                    .unwrap()
                    .previous_block_hash;
            } else {
                break;
            }
        }

        // and get existing current chain for comparison
        if shared_ancestor_found {
            trace!("shared ancestor found");

            while new_chain_hash != old_chain_hash {
                if self.blocks.contains_key(&old_chain_hash) {
                    old_chain.push(old_chain_hash);
                    old_chain_hash = self
                        .blocks
                        .get(&old_chain_hash)
                        .unwrap()
                        .previous_block_hash;
                    if old_chain_hash == [0; 32] {
                        break;
                    }
                } else {
                    break;
                }
            }
        } else {
            debug!(
                "block without parent. block : {:?}, latest : {:?}",
                block_hash.to_hex(),
                previous_block_hash.to_hex()
            );

            //
            // we have a block without a parent.
            //
            // if this is our first block, the blockring will have no entry yet
            // and block_ring_lc_pos (longest_chain_position) will be pointing
            // at None. We use this to determine if we are a new chain instead
            // of creating a separate variable to manually track entries.
            //
            if self.blockring.is_empty() {

                //
                // no need for action as fall-through will result in proper default
                // behavior. we have the comparison here to separate expected from
                // unexpected / edge-case issues around block receipt.
                //
            } else {
                //
                // TODO - implement logic to handle once nodes can connect
                //
                // if this not our first block, handle edge-case around receiving
                // block 503 before block 453 when block 453 is our expected proper
                // next block and we are getting blocks out-of-order because of
                // connection or network issues.

                if previous_block_hash != [0; 32]
                    && previous_block_hash == self.get_latest_block_hash()
                {
                    info!("blocks received out-of-order issue. handling edge case...");

                    let disconnected_block_id = self.get_latest_block_id();
                    for i in block_id + 1..disconnected_block_id {
                        let disconnected_block_hash =
                            self.blockring.get_longest_chain_block_hash_at_block_id(i);
                        if disconnected_block_hash != [0; 32] {
                            self.blockring.on_chain_reorganization(
                                i,
                                disconnected_block_hash,
                                false,
                            );
                            let disconnected_block = self.get_mut_block(&disconnected_block_hash);
                            if let Some(disconnected_block) = disconnected_block {
                                disconnected_block.in_longest_chain = false;
                            }
                        }
                    }

                    new_chain.clear();
                    new_chain.push(block_hash);
                    am_i_the_longest_chain = true;
                }
            }
        }

        // at this point we should have a shared ancestor or not
        // find out whether this new block is claiming to require chain-validation
        if !am_i_the_longest_chain && self.is_new_chain_the_longest_chain(&new_chain, &old_chain) {
            am_i_the_longest_chain = true;
        }

        // now update blockring so it is not empty
        //
        // we do this down here instead of automatically on
        // adding a block, as we want to have the above check
        // for handling the edge-case of blocks received in the
        // wrong order. the longest_chain check also requires a
        // first-block-received check that is conducted against
        // the blockring.
        //
        self.blockring.empty = false;

        // validate
        //
        // blockchain validate "validates" the new_chain by unwinding the old
        // and winding the new, which calling validate on any new previously-
        // unvalidated blocks. When the longest-chain status of blocks changes
        // the function on_chain_reorganization is triggered in blocks and
        // with the BlockRing. We fail if the newly-preferred chain is not
        // viable.
        //
        return if am_i_the_longest_chain {
            debug!("this is the longest chain");
            self.blocks.get_mut(&block_hash).unwrap().in_longest_chain = true;

            let does_new_chain_validate = self
                .validate(
                    new_chain.as_slice(),
                    old_chain.as_slice(),
                    storage,
                    configs,
                    network,
                )
                .await;

            if does_new_chain_validate {
                self.add_block_success(
                    block_hash,
                    network,
                    storage,
                    mempool,
                    configs,
                    sender_to_miner.is_some(),
                )
                .await;

                let difficulty = self.blocks.get(&block_hash).unwrap().difficulty;

                if sender_to_miner.is_some() {
                    debug!("sending longest chain block added event to miner : hash : {:?} difficulty : {:?} channel_capacity : {:?}", block_hash.to_hex(), difficulty,sender_to_miner.as_ref().unwrap().capacity());
                    sender_to_miner
                        .unwrap()
                        .send(MiningEvent::LongestChainBlockAdded {
                            hash: block_hash,
                            difficulty,
                        })
                        .await
                        .unwrap();
                }

                AddBlockResult::BlockAdded
            } else {
                warn!(
                    "new chain doesn't validate with hash : {:?}",
                    block_hash.to_hex()
                );
                self.blocks.get_mut(&block_hash).unwrap().in_longest_chain = false;
                self.add_block_failure(&block_hash, mempool).await;
                AddBlockResult::FailedButRetry
            }
        } else {
            debug!("this is not the longest chain");
            self.add_block_success(
                block_hash,
                network,
                storage,
                mempool,
                configs,
                sender_to_miner.is_some(),
            )
            .await;
            AddBlockResult::BlockAdded
        };
    }

    pub async fn add_block_success(
        &mut self,
        block_hash: SaitoHash,
        network: Option<&Network>,
        storage: &mut Storage,
        mempool: &mut Mempool,
        configs: &(dyn Configuration + Send + Sync),
        notify_miner: bool,
    ) {
        debug!("add_block_success : {:?}", block_hash.to_hex());

        // print blockring longest_chain_block_hash infor
        if notify_miner {
            // we only print blockchain after block queue is added to reduce the clutter in logs
            self.print(10);
        }

        let mut block_id = 0;
        let mut block_type = BlockType::Pruned;
        let mut tx_count = 0;
        // save to disk
        if network.is_some() {
            let block = self.get_mut_block(&block_hash).unwrap();
            block_id = block.id;
            block_type = block.block_type;
            tx_count = block.transactions.len();
            if block.block_type != BlockType::Header && !configs.is_browser() {
                // TODO : this will have an impact when the block sizes are getting large or there are many forks. need to handle this
                storage.write_block_to_disk(block).await;
            } else if block.block_type == BlockType::Header {
                debug!(
                    "block : {:?} not written to disk as type : {:?}",
                    block.hash.to_hex(),
                    block.block_type
                );
            }
            network.unwrap().propagate_block(block, configs).await;
        }

        // TODO: clean up mempool - I think we shouldn't cleanup mempool here.
        //  because that's already happening in send_blocks_to_blockchain
        //  So who is in charge here?
        //  is send_blocks_to_blockchain calling add_block or
        //  is blockchain calling mempool.on_chain_reorganization?
        //
        {
            mempool
                .transactions
                .retain(|_, tx| tx.validate_against_utxoset(&self.utxoset));
            let block = self.get_mut_block(&block_hash).unwrap();
            // we calling delete_tx after removing invalidated txs, to make sure routing work is calculated after removing all the txs
            mempool.delete_transactions(&block.transactions);
        }

        // propagate block to network
        // TODO : notify other threads and propagate to other peers

        // {
        //     // TODO : no need to access block multiple times. combine with previous call in block save call
        //     let block = self.get_mut_block(&block_hash).await;
        // }

        // global_sender
        //     .send(GlobalEvent::BlockchainSavedBlock { hash: block_hash })
        //     .expect("error: BlockchainSavedBlock message failed to send");
        // trace!(" ... block save done:            {:?}", create_timestamp());

        // update_genesis_period and prune old data - MOVED to on_chain_reorganization()
        //
        // self.update_genesis_period().await;

        //
        // fork id  - MOVED to on_chain_reorganization()
        //
        // let fork_id = self.generate_fork_id(block_id);
        // self.set_fork_id(fork_id);

        // ensure pruning of next block OK will have the right CVs
        //
        if self.get_latest_block_id() > GENESIS_PERIOD {
            let pruned_block_hash = self.blockring.get_longest_chain_block_hash_at_block_id(
                self.get_latest_block_id() - GENESIS_PERIOD,
            );

            assert_ne!(pruned_block_hash, [0; 32]);

            //
            // TODO
            //
            // handle this more efficiently - we should be able to prepare the block
            // in advance so that this doesn't take up time in block production. we
            // need to generate_metadata_hashes so that the slips know the utxo_key
            // to use to check the utxoset.
            //
            {
                let pblock = self.get_mut_block(&pruned_block_hash).unwrap();
                pblock
                    .upgrade_block_to_block_type(BlockType::Full, storage, configs.is_browser())
                    .await;
            }
        }
        debug!(
            "block {:?} added successfully. type : {:?} tx count = {:?}",
            block_hash.to_hex(),
            block_type,
            tx_count
        );
        if network.is_some() {
            network
                .unwrap()
                .io_interface
                .send_interface_event(InterfaceEvent::BlockAddSuccess(block_hash, block_id));
        }
    }

    pub async fn add_block_failure(&mut self, block_hash: &SaitoHash, mempool: &mut Mempool) {
        info!("add block failed : {:?}", block_hash.to_hex());

        mempool.delete_block(block_hash);
        let block = self.blocks.remove(block_hash);

        if block.is_none() {
            error!(
                "block : {:?} is not found in blocks collection. couldn't handle block failure.",
                block_hash.to_hex()
            );
            return;
        }

        let mut block = block.unwrap();
        let public_key;
        {
            let (wallet, _wallet_) = lock_for_read!(mempool.wallet, LOCK_ORDER_WALLET);
            public_key = wallet.public_key;
        }
        if block.creator == public_key {
            let transactions = &mut block.transactions;
            let prev_count = transactions.len();
            let transactions: Vec<Transaction> = drain!(transactions, 10)
                .filter(|tx| {
                    // TODO : what other types should be added back to the mempool
                    if tx.transaction_type == TransactionType::Normal {
                        // TODO : is there a way to not validate these again ?
                        return tx.validate(&self.utxoset);
                    }
                    false
                })
                .collect();
            // transactions.retain(|tx| tx.validate(&self.utxoset));
            info!(
                "adding {:?} transactions back to mempool. dropped {:?} invalid transactions",
                transactions.len(),
                (prev_count - transactions.len())
            );
            for tx in transactions {
                mempool.transactions.insert(tx.signature, tx);
            }
            mempool.new_tx_added = true;
        }
    }

    pub fn generate_fork_id(&self, block_id: u64) -> SaitoHash {
        let mut fork_id = [0; 32];
        let mut current_block_id = block_id;

        // roll back to last even 10 blocks
        current_block_id = current_block_id - (current_block_id % 10);
        let weights = [
            0, 10, 10, 10, 10, 10, 25, 25, 100, 300, 500, 4000, 10000, 20000, 50000, 100000,
        ];

        // loop backwards through blockchain
        for (i, weight) in weights.iter().enumerate() {
            if current_block_id <= *weight {
                debug!(
                    "current_id : {:?} is less than weight : {:?}",
                    current_block_id, weight
                );
                break;
            }
            current_block_id -= weight;

            // index to update
            let index = 2 * i;

            let block_hash = self
                .blockring
                .get_longest_chain_block_hash_at_block_id(current_block_id);
            if block_hash != [0; 32] {
                fork_id[index] = block_hash[index];
                fork_id[index + 1] = block_hash[index + 1];
            }
        }

        fork_id
    }

    pub fn generate_last_shared_ancestor(
        &self,
        peer_latest_block_id: u64,
        fork_id: SaitoHash,
    ) -> u64 {
        let my_latest_block_id = self.get_latest_block_id();

        let mut peer_block_id = peer_latest_block_id;
        let mut my_block_id = my_latest_block_id;

        debug!(
            "generate last shared ancestor : peer_latest_id : {:?}, fork_id : {:?} my_latest_id : {:?}",
            peer_latest_block_id,
            fork_id.to_hex(),
            my_latest_block_id
        );
        let weights = [
            0, 10, 10, 10, 10, 10, 25, 25, 100, 300, 500, 4000, 10000, 20000, 50000, 100000,
        ];
        if peer_latest_block_id >= my_latest_block_id {
            // roll back to last even 10 blocks
            peer_block_id = peer_block_id - (peer_block_id % 10);
            debug!(
                "peer_block_id : {:?}, my_block_id : {:?}",
                peer_block_id, my_latest_block_id
            );

            // their fork id
            for (index, weight) in weights.iter().enumerate() {
                if peer_block_id < *weight {
                    debug!(
                        "peer_block_id : {:?} is less than weight : {:?}",
                        peer_block_id, weight
                    );
                    return 0;
                }
                peer_block_id -= weight;

                if peer_block_id >= my_block_id {
                    continue;
                }

                // index in fork_id hash
                let index = 2 * index;

                // compare input hash to my hash
                let block_hash = self
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(peer_block_id);
                if fork_id[index] == block_hash[index]
                    && fork_id[index + 1] == block_hash[index + 1]
                {
                    return peer_block_id;
                }
            }
        } else {
            my_block_id = my_block_id - (my_block_id % 10);

            debug!("my_block_id after rounding : {:?}", my_block_id);
            for (index, weight) in weights.iter().enumerate() {
                if my_block_id < *weight {
                    debug!(
                        "my_block_id : {:?} is less than weight : {:?}",
                        my_block_id, weight
                    );
                    return 0;
                }
                my_block_id -= weight;

                // do not loop around if block id < 0
                if my_block_id > peer_latest_block_id {
                    debug!(
                        "my_block_id {:?} > peer_latest_block_id : {:?}",
                        my_block_id, peer_latest_block_id
                    );
                    continue;
                }

                // index in fork_id hash
                let index = 2 * index;

                // compare input hash to my hash
                let block_hash = self
                    .blockring
                    .get_longest_chain_block_hash_at_block_id(my_block_id);
                if fork_id[index] == block_hash[index]
                    && fork_id[index + 1] == block_hash[index + 1]
                {
                    return my_block_id;
                }
            }
        }

        debug!("no shared ancestor found. returning 0");
        // no match? return 0 -- no shared ancestor
        0
    }
    pub fn print(&self, count: u64) {
        let latest_block_id = self.get_latest_block_id();
        let mut current_id = latest_block_id;

        let mut min_id = 0;
        if latest_block_id > count {
            min_id = latest_block_id - count;
        }
        debug!("------------------------------------------------------");
        while current_id > 0 && current_id >= min_id {
            let hash = self
                .blockring
                .get_longest_chain_block_hash_at_block_id(current_id);
            if hash == [0; 32] {
                break;
            }
            debug!(
                "{} - {:?}",
                current_id,
                self.blockring
                    .get_longest_chain_block_hash_at_block_id(current_id)
                    .to_hex()
            );
            current_id -= 1;
        }
        debug!("------------------------------------------------------");
    }

    pub fn get_latest_block(&self) -> Option<&Block> {
        let block_hash = self.blockring.get_latest_block_hash();
        self.blocks.get(&block_hash)
    }

    pub fn get_latest_block_hash(&self) -> SaitoHash {
        self.blockring.get_latest_block_hash()
    }

    pub fn get_latest_block_id(&self) -> u64 {
        self.blockring.get_latest_block_id()
    }

    pub fn get_block_sync(&self, block_hash: &SaitoHash) -> Option<&Block> {
        self.blocks.get(block_hash)
    }

    pub fn get_block(&self, block_hash: &SaitoHash) -> Option<&Block> {
        //

        self.blocks.get(block_hash)
    }

    pub fn get_mut_block(&mut self, block_hash: &SaitoHash) -> Option<&mut Block> {
        //
        self.blocks.get_mut(block_hash)
    }

    pub fn is_block_indexed(&self, block_hash: SaitoHash) -> bool {
        if self.blocks.contains_key(&block_hash) {
            return true;
        }
        false
    }

    pub fn contains_block_hash_at_block_id(&self, block_id: u64, block_hash: SaitoHash) -> bool {
        self.blockring
            .contains_block_hash_at_block_id(block_id, block_hash)
    }

    pub fn is_new_chain_the_longest_chain(
        &self,
        new_chain: &[SaitoHash],
        old_chain: &[SaitoHash],
    ) -> bool {
        trace!("checking for longest chain");
        if self.blockring.is_empty() {
            return true;
        }
        if old_chain.len() > new_chain.len() {
            warn!(
                "WARN: old chain length : {:?} is greater than new chain length : {:?}",
                old_chain.len(),
                new_chain.len()
            );
            return false;
        }

        if self.blockring.get_latest_block_id() >= self.blocks.get(&new_chain[0]).unwrap().id {
            return false;
        }

        let mut old_bf: Currency = 0;
        let mut new_bf: Currency = 0;

        for hash in old_chain.iter() {
            old_bf += self.blocks.get(hash).unwrap().burnfee;
        }
        for hash in new_chain.iter() {
            if let Some(x) = self.blocks.get(hash) {
                new_bf += x.burnfee;
            } else {
                return false;
            }
            //new_bf += self.blocks.get(hash).unwrap().get_burnfee();
        }
        // new chain must have more accumulated work AND be longer
        //
        old_chain.len() < new_chain.len() && old_bf <= new_bf
    }

    //
    // when new_chain and old_chain are generated the block_hashes are added
    // to their vectors from tip-to-shared-ancestors. if the shared ancestors
    // is at position [0] in our blockchain for instance, we may receive:
    //
    // new_chain --> adds the hashes in this order
    //   [5] [4] [3] [2] [1]
    //
    // old_chain --> adds the hashes in this order
    //   [4] [3] [2] [1]
    //
    // unwinding requires starting from the BEGINNING of the vector, while
    // winding requires starting from th END of the vector. the loops move
    // in opposite directions.
    //
    pub async fn validate(
        &mut self,
        new_chain: &[SaitoHash],
        old_chain: &[SaitoHash],
        storage: &Storage,
        configs: &(dyn Configuration + Send + Sync),
        network: Option<&Network>,
    ) -> bool {
        debug!("validating chains");

        let previous_block_hash;
        let has_gt;
        {
            let block = self.blocks.get(new_chain[0].as_ref()).unwrap();
            previous_block_hash = block.previous_block_hash;
            has_gt = block.has_golden_ticket;
        }
        // ensure new chain has adequate mining support to be considered as
        // a viable chain. we handle this check here as opposed to handling
        // it in wind_chain as we only need to check once for the entire chain
        //
        if !self.is_golden_ticket_count_valid(
            previous_block_hash,
            has_gt,
            configs.is_browser(),
            configs.is_spv_mode(),
        ) {
            return false;
        }

        if old_chain.is_empty() {
            self.wind_chain(
                new_chain,
                old_chain,
                new_chain.len() - 1,
                false,
                storage,
                configs.deref(),
                network,
            )
            .await
        } else if !new_chain.is_empty() {
            self.unwind_chain(new_chain, old_chain, 0, true, storage, configs, network)
                .await
        } else {
            warn!("lengths are inappropriate");
            false
        }
    }

    pub fn is_golden_ticket_count_valid(
        &self,
        previous_block_hash: SaitoHash,
        current_block_has_golden_ticket: bool,
        is_browser: bool,
        is_spv: bool,
    ) -> bool {
        let mut golden_tickets_found = 0;
        let mut search_depth_index = 0;
        let mut latest_block_hash = previous_block_hash;

        for i in 0..MIN_GOLDEN_TICKETS_DENOMINATOR {
            search_depth_index += 1;

            if let Some(block) = self.get_block_sync(&latest_block_hash) {
                if i == 0 && block.id < MIN_GOLDEN_TICKETS_DENOMINATOR {
                    golden_tickets_found = MIN_GOLDEN_TICKETS_DENOMINATOR;
                    break;
                }

                // the latest block will not have has_golden_ticket set yet
                // so it is possible we undercount the latest block. this
                // is dealt with by manually checking for the existence of
                // a golden ticket if we only have 1 golden ticket below.
                if block.has_golden_ticket {
                    golden_tickets_found += 1;
                }
                latest_block_hash = block.previous_block_hash;
            } else {
                break;
            }
        }

        if golden_tickets_found < MIN_GOLDEN_TICKETS_NUMERATOR
            && search_depth_index >= MIN_GOLDEN_TICKETS_DENOMINATOR
            && current_block_has_golden_ticket
        {
            golden_tickets_found += 1;
        }

        if golden_tickets_found < MIN_GOLDEN_TICKETS_NUMERATOR
            && search_depth_index >= MIN_GOLDEN_TICKETS_DENOMINATOR
        {
            info!(
                "not enough golden tickets : found = {:?} depth = {:?}",
                golden_tickets_found, search_depth_index
            );
            // TODO : browsers might want to implement this check somehow
            if !is_browser && !is_spv {
                return false;
            }
        }
        true
    }

    //
    // when new_chain and old_chain are generated the block_hashes are added
    // to their vectors from tip-to-shared-ancestors. if the shared ancestors
    // is at position [0] for instance, we may receive:
    //
    // new_chain --> adds the hashes in this order
    //   [5] [4] [3] [2] [1]
    //
    // old_chain --> adds the hashes in this order
    //   [4] [3] [2] [1]
    //
    // unwinding requires starting from the BEGINNING of the vector, while
    // winding requires starting from the END of the vector. the loops move
    // in opposite directions. the argument current_wind_index is the
    // position in the vector NOT the ordinal number of the block_hash
    // being processed. we start winding with current_wind_index 4 not 0.
    //
    #[async_recursion]
    pub async fn wind_chain(
        &mut self,
        new_chain: &[SaitoHash],
        old_chain: &[SaitoHash],
        current_wind_index: usize,
        wind_failure: bool,
        storage: &Storage,
        configs: &(dyn Configuration + Send + Sync),
        network: Option<&'async_recursion Network>,
    ) -> bool {
        // trace!(" ... blockchain.wind_chain strt: {:?}", create_timestamp());

        // if we are winding a non-existent chain with a wind_failure it
        // means our wind attempt failed and we should move directly into
        // add_block_failure() by returning false.
        if wind_failure && new_chain.is_empty() {
            return false;
        }

        // winding the chain requires us to have certain data associated
        // with the block and the transactions, particularly the tx hashes
        // that we need to generate the slip UUIDs and create the tx sigs.
        //
        // we fetch the block mutably first in order to update these vars.
        // we cannot just send the block mutably into our regular validate()
        // function because of limitatins imposed by Rust on mutable data
        // structures. So validation is "read-only" and our "write" actions
        // happen first.
        let block_hash = new_chain.get(current_wind_index).unwrap();

        {
            let block = self.get_mut_block(block_hash).unwrap();

            block
                .upgrade_block_to_block_type(BlockType::Full, storage, configs.is_browser())
                .await;

            let latest_block_id = block.id;

            //
            // ensure previous blocks that may be needed to calculate the staking
            // tables or the nolan that are potentially falling off the chain have
            // full access to their transaction data.
            //
            for i in 1..MAX_STAKER_RECURSION {
                if i >= latest_block_id {
                    break;
                }
                let bid = latest_block_id - i;
                let previous_block_hash =
                    self.blockring.get_longest_chain_block_hash_at_block_id(bid);
                if self.is_block_indexed(previous_block_hash) {
                    let block = self.get_mut_block(&previous_block_hash).unwrap();
                    block
                        .upgrade_block_to_block_type(BlockType::Full, storage, configs.is_browser())
                        .await;
                }
            }
        }

        let block = self.blocks.get(block_hash).unwrap();
        // assert_eq!(block.block_type, BlockType::Full);

        let does_block_validate = block.validate(self, &self.utxoset, configs).await;

        if does_block_validate {
            // blockring update
            self.blockring
                .on_chain_reorganization(block.id, block.hash, true);

            //
            // TODO - wallet update should be optional, as core routing nodes
            // will not want to do the work of scrolling through the block and
            // updating their wallets by default. wallet processing can be
            // more efficiently handled by lite-nodes.
            //
            {
                // trace!(" ... wallet processing start:    {}", create_timestamp());
                let (mut wallet, _wallet_) = lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);

                wallet.on_chain_reorganization(block, true, network);

                // trace!(" ... wallet processing stop:     {}", create_timestamp());
            }
            let block_id = block.id;
            // let block_hash = block.hash;
            // utxoset update
            {
                let block = self.blocks.get_mut(block_hash).unwrap();
                block.on_chain_reorganization(&mut self.utxoset, true);
            }

            self.on_chain_reorganization(block_id, *block_hash, true, storage, configs, network)
                .await;

            //
            // we have received the first entry in new_blocks() which means we
            // have added the latest tip. if the variable wind_failure is set
            // that indicates that we ran into an issue when winding the new_chain
            // and what we have just processed is the old_chain (being rewound)
            // so we should exit with failure.
            //
            // otherwise we have successfully wound the new chain, and exit with
            // success.
            //
            if current_wind_index == 0 {
                if wind_failure {
                    return false;
                }
                return true;
            }

            let res = self
                .wind_chain(
                    new_chain,
                    old_chain,
                    current_wind_index - 1,
                    false,
                    storage,
                    configs,
                    network,
                )
                .await;
            res
        } else {
            //
            // we have had an error while winding the chain. this requires us to
            // unwind any blocks we have already wound, and rewind any blocks we
            // have unwound.
            //
            // we set wind_failure to "true" so that when we reach the end of
            // the process of rewinding the old-chain, our wind_chain function
            // will know it has rewound the old chain successfully instead of
            // successfully added the new chain.
            //
            error!(
                "ERROR: this block : {:?} does not validate!",
                block.hash.to_hex()
            );
            if current_wind_index == new_chain.len() - 1 {
                //
                // this is the first block we have tried to add
                // and so we can just roll out the older chain
                // again as it is known good.
                //
                // note that old and new hashes are swapped
                // and the old chain is set as null because
                // we won't move back to it. we also set the
                // resetting_flag to 1 so we know to fork
                // into addBlockToBlockchainFailure
                //
                // true -> force -> we had issues, is failure
                //
                // new_chain --> hashes are still in this order
                //   [5] [4] [3] [2] [1]
                //
                // we are at the beginning of our own vector so we have nothing
                // to unwind. Because of this, we start WINDING the old chain back
                // which requires us to start at the END of the new chain vector.
                //
                if !old_chain.is_empty() {
                    info!("old chain len: {}", old_chain.len());
                    let res = self
                        .wind_chain(
                            old_chain,
                            new_chain,
                            old_chain.len() - 1,
                            true,
                            storage,
                            configs,
                            network,
                        )
                        .await;
                    res
                } else {
                    false
                }
            } else {
                let mut chain_to_unwind: Vec<[u8; 32]> = vec![];

                //
                // if we run into a problem winding our chain after we have
                // wound any blocks, we take the subset of the blocks we have
                // already pushed through on_chain_reorganization (i.e. not
                // including this block!) and put them onto a new vector we
                // will unwind in turn.
                //
                for i in current_wind_index + 1..new_chain.len() {
                    chain_to_unwind.push(new_chain[i]);
                }

                //
                // chain to unwind is now something like this...
                //
                //  [3] [2] [1]
                //
                // unwinding starts from the BEGINNING of the vector
                //
                let res = self
                    .unwind_chain(
                        old_chain,
                        &chain_to_unwind,
                        0,
                        true,
                        storage,
                        configs,
                        network,
                    )
                    .await;
                res
            }
        }
    }

    //
    // when new_chain and old_chain are generated the block_hashes are pushed
    // to their vectors from tip-to-shared-ancestors. if the shared ancestors
    // is at position [0] for instance, we may receive:
    //
    // new_chain --> adds the hashes in this order
    //   [5] [4] [3] [2] [1]
    //
    // old_chain --> adds the hashes in this order
    //   [4] [3] [2] [1]
    //
    // unwinding requires starting from the BEGINNING of the vector, while
    // winding requires starting from the END of the vector. the first
    // block we have to remove in the old_chain is thus at position 0, and
    // walking up the vector from there until we reach the end.
    //
    #[async_recursion]
    pub async fn unwind_chain(
        &mut self,
        new_chain: &[SaitoHash],
        old_chain: &[SaitoHash],
        current_unwind_index: usize,
        wind_failure: bool,
        storage: &Storage,
        configs: &(dyn Configuration + Send + Sync),
        network: Option<&'async_recursion Network>,
    ) -> bool {
        let block_id;
        let block_hash;
        {
            let block = self
                .blocks
                .get_mut(&old_chain[current_unwind_index])
                .unwrap();
            block
                .upgrade_block_to_block_type(BlockType::Full, storage, configs.is_browser())
                .await;
            block_id = block.id;
            block_hash = block.hash;

            // utxoset update
            block.on_chain_reorganization(&mut self.utxoset, false);

            // blockring update
            self.blockring
                .on_chain_reorganization(block.id, block.hash, false);

            // wallet update
            {
                let (mut wallet, _wallet_) = lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);
                wallet.on_chain_reorganization(&block, false, network);
            }
        }
        self.on_chain_reorganization(block_id, block_hash, false, storage, configs, network)
            .await;
        if current_unwind_index == old_chain.len() - 1 {
            //
            // start winding new chain
            //
            // new_chain --> adds the hashes in this order
            //   [5] [4] [3] [2] [1]
            //
            // old_chain --> adds the hashes in this order
            //   [4] [3] [2] [1]
            //
            // winding requires starting at the END of the vector and rolling
            // backwards until we have added block #5, etc.
            //
            let res = self
                .wind_chain(
                    new_chain,
                    old_chain,
                    new_chain.len() - 1,
                    wind_failure,
                    storage,
                    configs,
                    network,
                )
                .await;
            res
        } else {
            //
            // continue unwinding,, which means
            //
            // unwinding requires moving FORWARD in our vector (and backwards in
            // the blockchain). So we increment our unwind index.
            //
            let res = self
                .unwind_chain(
                    new_chain,
                    old_chain,
                    current_unwind_index + 1,
                    wind_failure,
                    storage,
                    configs,
                    network,
                )
                .await;
            res
        }
    }

    /// keeps any blockchain variables like fork_id or genesis_period
    /// tracking variables updated as the chain gets new blocks. also
    /// pre-loads any blocks needed to improve performance.
    async fn on_chain_reorganization(
        &mut self,
        block_id: u64,
        block_hash: SaitoHash,
        longest_chain: bool,
        storage: &Storage,
        configs: &(dyn Configuration + Send + Sync),
        network: Option<&Network>,
    ) {
        debug!(
            "on_chain_reorganization : block_id = {:?} block_hash = {:?}",
            block_id,
            block_hash.to_hex()
        );
        // skip out if earlier than we need to be vis-a-vis last_block_id
        if self.last_block_id >= block_id {
            debug!(
                "last block id : {:?} is later than this block id : {:?}. skipping reorg",
                self.last_block_id, block_id
            );
            self.downgrade_blockchain_data(configs.is_browser()).await;
            return;
        }

        if longest_chain {
            let block = self.blocks.get(&block_hash);
            if let Some(block) = block {
                self.last_block_id = block_id;
                self.last_block_hash = block.hash;
                self.last_timestamp = block.timestamp;
                self.last_burnfee = block.burnfee;

                if self.lowest_acceptable_timestamp == 0 {
                    self.lowest_acceptable_block_id = block_id;
                    self.lowest_acceptable_block_hash = block.hash;
                    self.lowest_acceptable_timestamp = block.timestamp;
                }
            } else {
                warn!("block not found for hash : {:?}", block_hash.to_hex());
            }

            // update genesis period, purge old data
            self.update_genesis_period(storage, network).await;

            // generate fork_id
            let fork_id = self.generate_fork_id(block_id);
            self.set_fork_id(fork_id);
        }

        self.downgrade_blockchain_data(configs.is_browser()).await;
    }

    pub async fn update_genesis_period(&mut self, storage: &Storage, network: Option<&Network>) {
        // we need to make sure this is not a random block that is disconnected
        // from our previous genesis_id. If there is no connection between it
        // and us, then we cannot delete anything as otherwise the provision of
        // the block may be an attack on us intended to force us to discard
        // actually useful data.
        //
        // so we check that our block is the head of the longest-chain and only
        // update the genesis period when that is the case.
        //
        let latest_block_id = self.get_latest_block_id();
        if latest_block_id >= ((GENESIS_PERIOD * 2) + 1) {
            //
            // prune blocks
            //
            let purge_bid = latest_block_id - (GENESIS_PERIOD * 2);
            self.genesis_block_id = latest_block_id - GENESIS_PERIOD;
            debug!("genesis block id set as : {:?}", self.genesis_block_id);

            //
            // in either case, we are OK to throw out everything below the
            // lowest_block_id that we have found. we use the purge_id to
            // handle purges.
            if purge_bid > 0 {
                self.delete_blocks(purge_bid, storage, network).await;
            }
        }

        //TODO: we already had in update_genesis_period() in self method - maybe no need to call here?
        // self.downgrade_blockchain_data().await;
    }

    //
    // deletes all blocks at a single block_id
    //
    pub async fn delete_blocks(
        &mut self,
        delete_block_id: u64,
        storage: &Storage,
        network: Option<&Network>,
    ) {
        trace!(
            "removing data including from disk at id {}",
            delete_block_id
        );

        let mut block_hashes_copy: Vec<SaitoHash> = vec![];

        {
            let block_hashes = self.blockring.get_block_hashes_at_block_id(delete_block_id);
            for hash in block_hashes {
                block_hashes_copy.push(hash);
            }
        }

        trace!("number of hashes to remove {}", block_hashes_copy.len());

        for hash in block_hashes_copy {
            self.delete_block(delete_block_id, hash, storage, network)
                .await;
        }
    }

    //
    // deletes a single block
    //
    pub async fn delete_block(
        &mut self,
        delete_block_id: u64,
        delete_block_hash: SaitoHash,
        storage: &Storage,
        network: Option<&Network>,
    ) {
        //
        // ask block to delete itself / utxo-wise
        //
        {
            let pblock = self.blocks.get(&delete_block_hash).unwrap();
            let pblock_filename = storage.generate_block_filepath(pblock);

            //
            // remove slips from wallet
            //
            {
                let (mut wallet, _wallet_) = lock_for_write!(self.wallet_lock, LOCK_ORDER_WALLET);

                wallet.delete_block(pblock, network);
            }
            //
            // removes utxoset data
            //
            pblock.delete(&mut self.utxoset).await;

            //
            // deletes block from disk
            //
            storage.delete_block_from_disk(pblock_filename).await;
        }

        //
        // ask blockring to remove
        //
        self.blockring
            .delete_block(delete_block_id, delete_block_hash);

        //
        // remove from block index
        //
        if self.blocks.contains_key(&delete_block_hash) {
            self.blocks.remove_entry(&delete_block_hash);
        }
    }

    pub async fn downgrade_blockchain_data(&mut self, is_browser: bool) {
        trace!("downgrading blockchain data");
        //
        // downgrade blocks still on the chain
        //
        if PRUNE_AFTER_BLOCKS > self.get_latest_block_id() {
            return;
        }
        let prune_blocks_at_block_id = self.get_latest_block_id() - PRUNE_AFTER_BLOCKS;

        let mut block_hashes_copy: Vec<SaitoHash> = vec![];

        {
            let block_hashes = self
                .blockring
                .get_block_hashes_at_block_id(prune_blocks_at_block_id);
            for hash in block_hashes {
                block_hashes_copy.push(hash);
            }
        }

        for hash in block_hashes_copy {
            // ask the block to remove its transactions
            {
                let block = self.get_mut_block(&hash);
                if let Some(block) = block {
                    block
                        .downgrade_block_to_block_type(BlockType::Pruned, is_browser)
                        .await;
                } else {
                    warn!("block : {:?} not found to downgrade", hash.to_hex());
                }
            }
        }
    }
    pub async fn add_blocks_from_mempool(
        &mut self,
        mempool_lock: Arc<RwLock<Mempool>>,
        network: Option<&Network>,
        storage: &mut Storage,
        sender_to_miner: Option<Sender<MiningEvent>>,
        configs: &(dyn Configuration + Send + Sync),
    ) -> bool {
        debug!("adding blocks from mempool to blockchain");
        let mut blocks: VecDeque<Block>;
        let (mut mempool, _mempool_) = lock_for_write!(mempool_lock, LOCK_ORDER_MEMPOOL);

        blocks = mempool.blocks_queue.drain(..).collect();
        blocks.make_contiguous().sort_by(|a, b| a.id.cmp(&b.id));

        debug!("blocks to add : {:?}", blocks.len());
        let mut blockchain_updated = false;
        while let Some(block) = blocks.pop_front() {
            let result = self
                .add_block(
                    block,
                    network,
                    storage,
                    sender_to_miner.clone(),
                    &mut mempool,
                    configs.deref(),
                )
                .await;
            if !blockchain_updated {
                if let AddBlockResult::BlockAdded = result {
                    blockchain_updated = true;
                }
            }
        }

        debug!(
            "added blocks to blockchain. added back : {:?}",
            mempool.blocks_queue.len()
        );
        blockchain_updated
    }
    pub fn add_ghost_block(
        &mut self,
        id: u64,
        previous_block_hash: SaitoHash,
        ts: Timestamp,
        prehash: SaitoHash,
        gt: bool,
        hash: SaitoHash,
    ) {
        let mut block = Block::new();
        block.id = id;
        block.previous_block_hash = previous_block_hash;
        block.timestamp = ts;
        block.has_golden_ticket = gt;
        block.pre_hash = prehash;
        block.hash = hash;
        block.block_type = BlockType::Ghost;

        if self.is_block_indexed(hash) {
            warn!("block :{:?} exists in blockchain", hash.to_hex());
            return;
        }
        if !self.blockring.contains_block_hash_at_block_id(id, hash) {
            self.blockring.add_block(&block);
        }
        if !self.is_block_indexed(hash) {
            self.blocks.insert(hash, block);
        }
    }
    pub async fn reset(&mut self) {
        self.last_burnfee = 0;
        self.last_timestamp = 0;
        self.last_block_id = 0;
        self.last_block_hash = [0; 32];
        self.genesis_timestamp = 0;
        self.genesis_block_hash = [0; 32];
        self.genesis_block_id = 0;
        self.lowest_acceptable_block_id = 0;
        self.lowest_acceptable_timestamp = 0;
        self.lowest_acceptable_block_hash = [0; 32];
        self.save().await;
    }

    pub async fn save(&self) {
        // TODO : what should be done here in rust code?
    }
    pub fn get_utxoset_data(&self) -> HashMap<SaitoPublicKey, Currency> {
        let mut data: HashMap<SaitoPublicKey, Currency> = Default::default();
        self.utxoset.iter().for_each(|(key, value)| {
            if !value {
                return;
            }
            let slip = Slip::parse_slip_from_utxokey(key);
            *data.entry(slip.public_key).or_default() += slip.amount;
            // data.insert(slip.public_key, slip.amount);
        });
        data
    }
    pub fn get_slips_for(&self, public_key: SaitoPublicKey) -> Vec<Slip> {
        let mut slips: Vec<Slip> = Default::default();
        self.utxoset
            .iter()
            .filter(|(_, value)| **value)
            .for_each(|(key, _)| {
                let slip = Slip::parse_slip_from_utxokey(key);
                if slip.public_key == public_key {
                    slips.push(slip);
                }
            });
        slips
    }
    pub fn get_balance_snapshot(&self, keys: Vec<SaitoPublicKey>) -> BalanceSnapshot {
        let mut snapshot = BalanceSnapshot {
            latest_block_id: self.get_latest_block_id(),
            latest_block_hash: self.get_latest_block_hash(),
            timestamp: self.last_timestamp,
            slips: vec![],
        };
        // TODO : calling this will be a huge performance hit for the node. so need to refactor the design.
        self.utxoset
            .iter()
            .filter(|(_, value)| **value)
            .for_each(|(key, _)| {
                let slip = Slip::parse_slip_from_utxokey(key);

                // if no keys provided we get the full picture
                if keys.is_empty() || keys.contains(&slip.public_key) {
                    snapshot.slips.push(slip);
                }
            });

        snapshot
    }
    pub fn mark_as_fetching(&mut self, block_hash: SaitoHash) {
        debug!("marking block : {:?} as fetching", block_hash.to_hex());
        self.blocks_fetching.insert(block_hash);
    }
    pub fn unmark_as_fetching(&mut self, block_hash: &SaitoHash) {
        debug!("unmarking block : {:?} as fetching", block_hash.to_hex());
        self.blocks_fetching.remove(block_hash);
    }
    pub fn is_block_fetching(&self, block_hash: &SaitoHash) -> bool {
        self.blocks_fetching.contains(block_hash)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::Deref;
    use std::sync::Arc;

    use log::{debug, error, info};
    use tokio::sync::RwLock;

    use crate::common::defs::{
        push_lock, PrintForLog, SaitoPublicKey, LOCK_ORDER_BLOCKCHAIN, LOCK_ORDER_CONFIGS,
        LOCK_ORDER_WALLET,
    };
    use crate::common::test_manager::test::TestManager;
    use crate::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
    use crate::core::data::crypto::generate_keys;
    use crate::core::data::slip::Slip;
    use crate::core::data::storage::Storage;
    use crate::core::data::wallet::Wallet;
    use crate::{lock_for_read, lock_for_write};

    // fn init_testlog() {
    //     let _ = pretty_env_logger::try_init();
    // }

    #[tokio::test]
    async fn test_blockchain_init() {
        let keys = generate_keys();

        let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
        let blockchain = Blockchain::new(wallet);

        assert_eq!(blockchain.fork_id, [0; 32]);
        assert_eq!(blockchain.genesis_block_id, 0);
    }

    #[tokio::test]
    async fn test_add_block() {
        let keys = generate_keys();
        let wallet = Arc::new(RwLock::new(Wallet::new(keys.1, keys.0)));
        let blockchain = Blockchain::new(wallet);

        assert_eq!(blockchain.fork_id, [0; 32]);
        assert_eq!(blockchain.genesis_block_id, 0);
    }

    #[test]
    //
    // code that packs/unpacks two 32-bit values into one 64-bit variable
    //
    fn bit_pack_test() {
        let top = 157171715;
        let bottom = 11661612;
        let packed = bit_pack(top, bottom);
        assert_eq!(packed, 157171715 * (u64::pow(2, 32)) + 11661612);
        let (new_top, new_bottom) = bit_unpack(packed);
        assert_eq!(top, new_top);
        assert_eq!(bottom, new_bottom);

        let top = u32::MAX;
        let bottom = u32::MAX;
        let packed = bit_pack(top, bottom);
        let (new_top, new_bottom) = bit_unpack(packed);
        assert_eq!(top, new_top);
        assert_eq!(bottom, new_bottom);

        let top = 0;
        let bottom = 1;
        let packed = bit_pack(top, bottom);
        let (new_top, new_bottom) = bit_unpack(packed);
        assert_eq!(top, new_top);
        assert_eq!(bottom, new_bottom);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn initialize_blockchain_test() {
        let mut t = TestManager::new();

        // create first block, with 100 VIP txs with 1_000_000_000 NOLAN each
        t.initialize(100, 1_000_000_000).await;
        t.wait_for_mining_event().await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            assert_eq!(1, blockchain.get_latest_block_id());
        }
        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    #[tokio::test]
    #[serial_test::serial]
    //
    // test we can produce five blocks in a row
    //
    async fn add_five_good_blocks() {
        // let filter = tracing_subscriber::EnvFilter::from_default_env();
        // let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);
        //
        // tracing_subscriber::registry().with(fmt_layer).init();

        let mut t = TestManager::new();
        let block1;
        let block1_id;
        let block1_hash;
        let ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);
            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.id;
            block1_hash = block1.hash;
            ts = block1.timestamp;

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.hash;
        let block2_id = block2.id;

        t.add_block(block2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.hash;
        let block3_id = block3.id;

        t.add_block(block3).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.hash;
        let block4_id = block4.id;

        t.add_block(block4).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.hash;
        let block5_id = block5.id;

        t.add_block(block5).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;

        {
            let (wallet, _wallet_) = lock_for_read!(t.wallet_lock, LOCK_ORDER_WALLET);
            let count = wallet.get_unspent_slip_count();
            assert_ne!(count, 0);
            let balance = wallet.get_available_balance();
            assert_ne!(balance, 0);
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    //
    // test we do not add blocks because of insufficient mining
    //
    async fn insufficient_golden_tickets_test() {
        // let filter = tracing_subscriber::EnvFilter::from_default_env();
        // let fmt_layer = tracing_subscriber::fmt::Layer::default().with_filter(filter);
        //
        // tracing_subscriber::registry().with(fmt_layer).init();

        let mut t = TestManager::new();
        let block1;
        let block1_id;
        let block1_hash;
        let ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.id;
            block1_hash = block1.hash;
            ts = block1.timestamp;

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.hash;
        let block2_id = block2.id;

        t.add_block(block2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.hash;
        let block3_id = block3.id;

        t.add_block(block3).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.hash;
        let block4_id = block4.id;

        t.add_block(block4).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.hash;
        let block5_id = block5.id;

        t.add_block(block5).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        //
        // block 6
        //
        let mut block6 = t
            .create_block(
                block5_hash, // hash of parent block
                ts + 600000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block6.generate(); // generate hashes

        let block6_hash = block6.hash;
        let block6_id = block6.id;

        t.add_block(block6).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(
                blockchain.get_latest_block_hash().to_hex(),
                block3_hash.to_hex()
            );
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(
                blockchain.get_latest_block_hash().to_hex(),
                block4_hash.to_hex()
            );
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(
                blockchain.get_latest_block_hash().to_hex(),
                block5_hash.to_hex()
            );
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_hash(), block6_hash);
            assert_eq!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_id(), 6);
        }

        //
        // block 7
        //
        let mut block7 = t
            .create_block(
                block6_hash, // hash of parent block
                ts + 720000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block7.generate(); // generate hashes

        let block7_hash = block7.hash;
        let block7_id = block7.id;

        t.add_block(block7).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(blockchain.get_latest_block_hash(), block5_hash);
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_hash(), block6_hash);
            assert_eq!(blockchain.get_latest_block_id(), block6_id);
            assert_ne!(blockchain.get_latest_block_hash(), block7_hash);
            assert_ne!(blockchain.get_latest_block_id(), block7_id);
            assert_eq!(blockchain.get_latest_block_id(), 6);
        }

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    // tests if utxo hashmap persists after a blockchain reset
    #[tokio::test]
    async fn balance_hashmap_persists_after_blockchain_reset_test() {
        let mut t: TestManager = TestManager::new();
        let file_path = t.issuance_path;
        let slips = t
            .storage
            .get_token_supply_slips_from_disk_path(file_path)
            .await;

        // start blockchain with existing issuance and some value to my public key
        t.initialize_from_slips_and_value(slips.clone(), 100000)
            .await;

        // add a few transactions
        let public_keys = [
            "s8oFPjBX97NC2vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR",
            // "s9adoFPjBX972vbm9E5Kd2oHWUShuSTUuZwSB1U4wsPR",
            // "s223oFPjBX97NC2bmE5Kd2oHWUShuSTUuZwSB1U4wsPR",
        ];

        let mut last_param = 120000;
        for &public_key_string in &public_keys {
            let public_key = Storage::decode_str(public_key_string).unwrap();
            let mut to_public_key: SaitoPublicKey = [0u8; 33];
            to_public_key.copy_from_slice(&public_key);
            t.transfer_value_to_public_key(to_public_key, 500, last_param)
                .await
                .unwrap();
            last_param += 120000;
        }

        // save utxo balance map on issuance file
        let balance_map = t.balance_map().await;
        match t
            .storage
            .write_utxoset_to_disk_path(balance_map.clone(), 1, file_path)
            .await
        {
            Ok(_) => {
                debug!("store file ok");
            }
            Err(e) => {
                error!("Error: {:?}", e);
            }
        }

        // reset blockchain
        let mut t: TestManager = TestManager::new();
        let slips = t
            .storage
            .get_token_supply_slips_from_disk_path(t.issuance_path)
            .await;

        let issuance_hashmap = t.convert_issuance_to_hashmap(t.issuance_path).await;

        // initialize from existing slips
        t.initialize_from_slips(slips.clone()).await;

        let balance_map_after_reset = t.balance_map().await;

        assert_eq!(issuance_hashmap, balance_map_after_reset);
    }

    // test we do not add blocks because of insufficient mining
    #[tokio::test]
    #[serial_test::serial]
    async fn seven_blocks_with_sufficient_golden_tickets_test() {
        // pretty_env_logger::init();
        let mut t = TestManager::new();
        let block1;
        let block1_id;
        let block1_hash;
        let ts;

        // block 1
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            block1 = blockchain.get_latest_block().unwrap();
            block1_hash = block1.hash;
            block1_id = block1.id;
            ts = block1.timestamp;

            assert_eq!(blockchain.get_latest_block_hash(), block1_hash);
            assert_eq!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_id(), 1);
            assert_eq!(block1.transactions.len(), 100);
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.hash;
        let block2_id = block2.id;

        t.add_block(block2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_id(), 2);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes

        let block3_hash = block3.hash;
        let block3_id = block3.id;

        t.add_block(block3).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_eq!(blockchain.get_latest_block_hash(), block3_hash);
            assert_eq!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_id(), 3);
        }

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes

        let block4_hash = block4.hash;
        let block4_id = block4.id;

        t.add_block(block4).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_eq!(blockchain.get_latest_block_hash(), block4_hash);
            assert_eq!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_id(), 4);
        }

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes

        let block5_hash = block5.hash;
        let block5_id = block5.id;

        t.add_block(block5).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_id(), 5);
        }

        //
        // block 6
        //
        let mut block6 = t
            .create_block(
                block5_hash, // hash of parent block
                ts + 600000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block6.generate(); // generate hashes

        let block6_hash = block6.hash;
        let block6_id = block6.id;

        t.add_block(block6).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(blockchain.get_latest_block_hash(), block5_hash);
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_eq!(blockchain.get_latest_block_hash(), block6_hash);
            assert_eq!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_id(), 6);
        }

        //
        // block 7
        //
        let mut block7 = t
            .create_block(
                block6_hash, // hash of parent block
                ts + 720000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block7.generate(); // generate hashes

        let block7_hash = block7.hash;
        let block7_id = block7.id;

        t.add_block(block7).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_ne!(blockchain.get_latest_block_hash(), block1_hash);
            assert_ne!(blockchain.get_latest_block_id(), block1_id);
            assert_ne!(blockchain.get_latest_block_hash(), block2_hash);
            assert_ne!(blockchain.get_latest_block_id(), block2_id);
            assert_ne!(blockchain.get_latest_block_hash(), block3_hash);
            assert_ne!(blockchain.get_latest_block_id(), block3_id);
            assert_ne!(blockchain.get_latest_block_hash(), block4_hash);
            assert_ne!(blockchain.get_latest_block_id(), block4_id);
            assert_ne!(blockchain.get_latest_block_hash(), block5_hash);
            assert_ne!(blockchain.get_latest_block_id(), block5_id);
            assert_ne!(blockchain.get_latest_block_hash(), block6_hash);
            assert_ne!(blockchain.get_latest_block_id(), block6_id);
            assert_eq!(blockchain.get_latest_block_hash(), block7_hash);
            assert_eq!(blockchain.get_latest_block_id(), block7_id);
            assert_eq!(blockchain.get_latest_block_id(), 7);
        }

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    #[tokio::test]
    #[serial_test::serial]
    //
    // add 6 blocks including 4 block reorg
    //
    async fn basic_longest_chain_reorg_test() {
        let mut t = TestManager::new();
        let block1;
        let block1_hash;
        let ts;

        //
        // block 1
        //
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            block1 = blockchain.get_latest_block().unwrap();
            block1_hash = block1.hash;
            ts = block1.timestamp;
        }

        //
        // block 2
        //
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.hash;
        let block2_id = block2.id;

        t.add_block(block2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block2_id);
        }

        //
        // block 3
        //
        let mut block3 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block3.generate(); // generate hashes
        let block3_hash = block3.hash;
        let _block3_id = block3.id;
        t.add_block(block3).await;

        //
        // block 4
        //
        let mut block4 = t
            .create_block(
                block3_hash, // hash of parent block
                ts + 360000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block4.generate(); // generate hashes
        let block4_hash = block4.hash;
        let _block4_id = block4.id;
        t.add_block(block4).await;

        //
        // block 5
        //
        let mut block5 = t
            .create_block(
                block4_hash, // hash of parent block
                ts + 480000, // timestamp
                1,           // num transactions
                0,           // amount
                0,           // fee
                false,       // mine golden ticket
            )
            .await;
        block5.generate(); // generate hashes
        let block5_hash = block5.hash;
        let block5_id = block5.id;
        t.add_block(block5).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
        }

        //
        //  block3-2
        //
        let mut block3_2 = t
            .create_block(
                block2_hash, // hash of parent block
                ts + 240000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block3_2.generate(); // generate hashes
        let block3_2_hash = block3_2.hash;
        let _block3_2_id = block3_2.id;
        t.add_block(block3_2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
        }

        //
        //  block4-2
        //
        let mut block4_2 = t
            .create_block(
                block3_2_hash, // hash of parent block
                ts + 360000,   // timestamp
                0,             // num transactions
                0,             // amount
                0,             // fee
                true,          // mine golden ticket
            )
            .await;
        block4_2.generate(); // generate hashes
        let block4_2_hash = block4_2.hash;
        let _block4_2_id = block4_2.id;
        t.add_block(block4_2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
        }

        //
        //  block5-2
        //
        let mut block5_2 = t
            .create_block(
                block4_2_hash, // hash of parent block
                ts + 480000,   // timestamp
                1,             // num transactions
                0,             // amount
                0,             // fee
                false,         // mine golden ticket
            )
            .await;
        block5_2.generate(); // generate hashes
        let block5_2_hash = block5_2.hash;
        let _block5_2_id = block5_2.id;
        t.add_block(block5_2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block5_hash);
            assert_eq!(blockchain.get_latest_block_id(), block5_id);
        }

        //
        //  block6_2
        //
        let mut block6_2 = t
            .create_block(
                block5_2_hash, // hash of parent block
                ts + 600000,   // timestamp
                0,             // num transactions
                0,             // amount
                0,             // fee
                true,          // mine golden ticket
            )
            .await;
        block6_2.generate(); // generate hashes
        let block6_2_hash = block6_2.hash;
        let block6_2_id = block6_2.id;
        t.add_block(block6_2).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            assert_eq!(blockchain.get_latest_block_hash(), block6_2_hash);
            assert_eq!(blockchain.get_latest_block_id(), block6_2_id);
            assert_eq!(blockchain.get_latest_block_id(), 6);
        }

        t.check_blockchain().await;
        t.check_utxoset().await;
        t.check_token_supply().await;
    }

    /// Loading blocks into a blockchain which were created from another blockchain instance
    #[tokio::test]
    #[serial_test::serial]
    async fn load_blocks_from_another_blockchain_test() {
        // pretty_env_logger::init();
        let mut t = TestManager::new();
        let mut t2 = TestManager::new();
        let block1;
        let block1_id;
        let block1_hash;
        let ts;

        // block 1
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            block1 = blockchain.get_latest_block().unwrap();
            block1_id = block1.id;
            block1_hash = block1.hash;
            ts = block1.timestamp;
        }

        // block 2
        let mut block2 = t
            .create_block(
                block1_hash, // hash of parent block
                ts + 120000, // timestamp
                0,           // num transactions
                0,           // amount
                0,           // fee
                true,        // mine golden ticket
            )
            .await;
        block2.generate(); // generate hashes

        let block2_hash = block2.hash;
        let _block2_id = block2.id;

        t.add_block(block2).await;

        let list = t2.storage.load_block_name_list().await.unwrap();
        t2.storage
            .load_blocks_from_disk(list, t2.mempool_lock.clone())
            .await;
        {
            let (configs, _configs_) = lock_for_read!(t2.configs, LOCK_ORDER_CONFIGS);
            let (mut blockchain2, _blockchain2_) =
                lock_for_write!(t2.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            blockchain2
                .add_blocks_from_mempool(
                    t2.mempool_lock.clone(),
                    Some(&t2.network),
                    &mut t2.storage,
                    Some(t2.sender_to_miner.clone()),
                    configs.deref(),
                )
                .await;
        }

        {
            let blockchain1 = t.blockchain_lock.read().await;
            let blockchain2 = t2.blockchain_lock.read().await;

            assert_eq!(blockchain1.blocks.len(), 2);
            assert_eq!(blockchain2.blocks.len(), 2);

            let block1_chain1 = blockchain1.get_block(&block1_hash).unwrap();
            let block1_chain2 = blockchain2.get_block(&block1_hash).unwrap();

            let block2_chain1 = blockchain1.get_block(&block2_hash).unwrap();
            let block2_chain2 = blockchain2.get_block(&block2_hash).unwrap();

            for (block_new, block_old) in [
                (block1_chain2, block1_chain1),
                (block2_chain2, block2_chain1),
            ] {
                assert_eq!(block_new.hash, block_old.hash);
                assert_eq!(block_new.has_golden_ticket, block_old.has_golden_ticket);
                assert_eq!(block_new.previous_block_hash, block_old.previous_block_hash);
                assert_eq!(block_new.block_type, block_old.block_type);
                assert_eq!(block_new.signature, block_old.signature);
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn fork_id_test() {
        // pretty_env_logger::init();

        let mut t = TestManager::new();
        let mut block1;
        let mut block1_id;
        let mut block1_hash;
        let mut ts;

        t.initialize_with_timestamp(100, 1_000_000_000, 10_000_000)
            .await;

        for _i in (0..20).step_by(1) {
            {
                let (blockchain, _blockchain_) =
                    lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

                block1 = blockchain.get_latest_block().unwrap();
                block1_hash = block1.hash;
                block1_id = block1.id;
                ts = block1.timestamp;
            }

            let mut block = t
                .create_block(
                    block1_hash, // hash of parent block
                    ts + 120000, // timestamp
                    0,           // num transactions
                    0,           // amount
                    0,           // fee
                    true,        // mine golden ticket
                )
                .await;
            block.generate(); // generate hashes

            let _block_hash = block.hash;
            let _block_id = block.id;

            t.add_block(block).await;

            let _result = t.receiver_in_miner.try_recv();
        }

        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let fork_id = blockchain.generate_fork_id(15);
            assert_ne!(fork_id, [0; 32]);
            assert_eq!(fork_id[2..], [0; 30]);

            let fork_id = blockchain.generate_fork_id(20);
            assert_ne!(fork_id, [0; 32]);
            assert_eq!(fork_id[4..], [0; 28]);
        }
    }

    //create a test genesis block and test store state and reload from the same file
    #[tokio::test]
    #[serial_test::serial]
    async fn test_genesis_inout() {
        //init_testlog();

        let mut t = TestManager::new();
        //generate a test genesis block
        t.create_test_gen_block(1000).await;
        {
            let (blockchain, _blockchain_) =
                lock_for_read!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            let block1 = blockchain.get_latest_block().unwrap();
            assert_eq!(block1.id, 1);
            assert!(block1.timestamp > 1687867265673);
            assert_eq!(block1.transactions.len(), 1);
        }

        //create the balance map
        let bmap = t.balance_map().await;

        //store it
        //TODO path errors
        let filepath = "./utxoset_test";

        match t
            .storage
            .write_utxoset_to_disk_path(bmap, 1, filepath)
            .await
        {
            Ok(_) => {
                debug!("store file ok");
            }
            Err(e) => {
                error!("Error: {:?}", e);
            }
        }

        //now assume the stored map is issued as issued, pass it in

        //convert_issuance_into_slip
        let slips: Vec<Slip> = t
            .storage
            .get_token_supply_slips_from_disk_path(filepath)
            .await;
        assert_eq!(slips.len(), 1);

        //TODO more tests on slips

        //clean up the testing file
        let _ = fs::remove_file(filepath);
    }

    #[ignore]
    #[tokio::test]
    #[serial_test::serial]
    async fn add_block_from_mempool_test() {
        // pretty_env_logger::init();
        let mut t = TestManager::new();
        let block1;
        let mut parent_block_hash;
        let mut parent_block_id;
        let mut ts;

        // block 1
        t.initialize(100, 1_000_000_000).await;

        {
            let (blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            block1 = blockchain.get_latest_block().unwrap();
            parent_block_hash = block1.hash;
            parent_block_id = block1.id;
            ts = block1.timestamp;
        }

        for i in 0..50 {
            // block 2
            let mut block2 = t
                .create_block(
                    parent_block_hash, // hash of parent block
                    ts + 120000,       // timestamp
                    10,                // num transactions
                    0,                 // amount
                    0,                 // fee
                    false,             // mine golden ticket
                )
                .await;
            block2.id = parent_block_id + 1;
            info!("block generate : {:?}", block2.id);
            block2.generate(); // generate hashes
            block2.sign(&t.wallet_lock.read().await.private_key);
            parent_block_hash = block2.hash;
            parent_block_id = block2.id;
            ts = block2.timestamp;

            let mut mempool = t.mempool_lock.write().await;
            mempool.add_block(block2);
        }

        {
            let (configs, _configs_) = lock_for_read!(t.configs, LOCK_ORDER_CONFIGS);
            let (mut blockchain, _blockchain_) =
                lock_for_write!(t.blockchain_lock, LOCK_ORDER_BLOCKCHAIN);

            blockchain
                .add_blocks_from_mempool(
                    t.mempool_lock.clone(),
                    Some(&t.network),
                    &mut t.storage,
                    Some(t.sender_to_miner.clone()),
                    configs.deref(),
                )
                .await;
        }

        {
            let blockchain = t.blockchain_lock.read().await;
            assert_eq!(blockchain.blocks.len(), 6);
        }
    }
}

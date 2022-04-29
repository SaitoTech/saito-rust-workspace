use std::io::Error;
use std::sync::Arc;

use ahash::AHashMap;
use async_recursion::async_recursion;
use log::{debug, error, info, trace, warn};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoUTXOSetKey};
use crate::common::handle_io::HandleIo;
use crate::core::data::block::{Block, BlockType};
use crate::core::data::blockring::BlockRing;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::staking::Staking;
use crate::core::data::storage::Storage;
use crate::core::data::transaction::TransactionType;
use crate::core::data::wallet::Wallet;
use crate::core::miner_controller::MinerEvent;

// length of 1 genesis period
pub const GENESIS_PERIOD: u64 = 10;
// prune blocks from index after N blocks
pub const PRUNE_AFTER_BLOCKS: u64 = 20;
// max recursion when paying stakers -- number of blocks including  -- number of blocks including GTT
pub const MAX_STAKER_RECURSION: u64 = 3;
// max token supply - used in validating block #1
pub const MAX_TOKEN_SUPPLY: u64 = 1_000_000_000_000_000_000;
// minimum golden tickets required ( NUMBER_OF_TICKETS / number of preceding blocks )
pub const MIN_GOLDEN_TICKETS_NUMERATOR: u64 = 2;
// minimum golden tickets required ( number of tickets / NUMBER_OF_PRECEDING_BLOCKS )
pub const MIN_GOLDEN_TICKETS_DENOMINATOR: u64 = 6;

pub fn bit_pack(top: u32, bottom: u32) -> u64 {
    ((top as u64) << 32) + (bottom as u64)
}

pub fn bit_unpack(packed: u64) -> (u32, u32) {
    // Casting from a larger integer to a smaller integer (e.g. u32 -> u8) will truncate, no need to mask this
    let bottom = packed as u32;
    let top = (packed >> 32) as u32;
    (top, bottom)
}

pub type UtxoSet = AHashMap<SaitoUTXOSetKey, u64>;

#[derive(Debug)]
pub struct Blockchain {
    pub staking: Staking,
    pub utxoset: UtxoSet,
    pub blockring: BlockRing,
    pub blocks: AHashMap<SaitoHash, Block>,
    pub wallet_lock: Arc<RwLock<Wallet>>,
    // broadcast_channel_sender: broadcast::Sender<GlobalEvent>,
    genesis_block_id: u64,
    fork_id: SaitoHash,
}

impl Blockchain {
    #[allow(clippy::new_without_default)]
    pub fn new(
        wallet_lock: Arc<RwLock<Wallet>>,
        // sender: tokio::sync::broadcast::Sender<GlobalEvent>,
    ) -> Self {
        Blockchain {
            staking: Staking::new(),
            utxoset: AHashMap::new(),
            blockring: BlockRing::new(),
            blocks: AHashMap::new(),
            wallet_lock,
            // broadcast_channel_sender: sender,
            genesis_block_id: 0,
            fork_id: [0; 32],
        }
    }
    pub fn init(&mut self) -> Result<(), Error> {
        Ok(())
    }

    pub fn set_fork_id(&mut self, fork_id: SaitoHash) {
        self.fork_id = fork_id;
    }

    pub fn get_fork_id(&self) -> SaitoHash {
        self.fork_id
    }

    #[async_recursion]
    pub async fn add_block(
        &mut self,
        mut block: Block,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        sender_to_miner: tokio::sync::mpsc::Sender<MinerEvent>,
    ) {
        debug!("adding block to blockchain");
        //
        // get missing block
        //
        if self.blockring.get_latest_block_id() > 0 {
            let mut earliest_block_id = 1;
            if self.get_latest_block_id() > GENESIS_PERIOD {
                earliest_block_id = self.get_latest_block_id() - GENESIS_PERIOD;
            }
            trace!("earliest_block_id {}", earliest_block_id);
            let earliest_block_hash = self
                .blockring
                .get_longest_chain_block_hash_by_block_id(earliest_block_id);
            trace!("earliest_block_hash {:?}", earliest_block_hash);
            let earliest_block = self.get_mut_block(&earliest_block_hash).await;

            if block.get_timestamp() > earliest_block.get_timestamp() {
                if self
                    .get_block(&block.get_previous_block_hash())
                    .await
                    .is_none()
                {
                    if block.get_id() > earliest_block_id {
                        if block.source_connection_id.is_some() {
                            // TODO : fetch the block here
                            let url;
                            {
                                let peers = peers.read().await;
                                let peer = peers.find_peer_by_address(
                                    &block.get_source_connection_id().unwrap(),
                                );

                                let peer = peer.unwrap();
                                url = peer.get_block_fetch_url(block.get_previous_block_hash());
                            }
                            let result = Block::fetch_missing_block(io_handler, url).await;
                            if result.is_err() {
                                warn!(
                                    "couldn't fetch block : {:?}",
                                    hex::encode(block.get_previous_block_hash())
                                );
                                todo!()
                            }

                            let result = result.unwrap();
                            self.add_block(
                                result,
                                io_handler,
                                peers.clone(),
                                sender_to_miner.clone(),
                            )
                            .await;
                            // global_sender
                            //     .send(GlobalEvent::MissingBlock {
                            //         peer_id: block.get_source_connection_id().unwrap(),
                            //         hash: block.get_previous_block_hash(),
                            //     })
                            //     .expect("error: MissingBlock message failed to send");
                        }
                    }
                }
            }
        }

        //
        // first things first, confirm hashes OK
        //
        block.generate_hashes();

        info!("add_block {}", &hex::encode(&block.get_hash()));
        // trace!(
        //     " ... blockchain.add_block start: {:?} hash: {:?}, and id {}",
        //     create_timestamp(),
        //     block.get_hash(),
        //     block.get_id()
        // );

        //
        // start by extracting some variables that we will use
        // repeatedly in the course of adding this block to the
        // blockchain and our various indices.
        //
        let block_hash = block.get_hash();
        let block_id = block.get_id();
        let previous_block_hash = self.blockring.get_latest_block_hash();

        //
        // sanity checks
        //
        if self.blocks.contains_key(&block_hash) {
            error!(
                "ERROR: block exists in blockchain {:?}",
                &hex::encode(&block.get_hash())
            );
            return;
        }

        //
        // pre-validation
        //
        // this would be a great place to put in a prevalidation check
        // once we are finished implementing Saito Classic. Goal would
        // be a fast form of lite-validation just to determine that it
        // is worth going through the more general effort of evaluating
        // this block for consensus.
        //

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
        if !self
            .blockring
            .contains_block_hash_at_block_id(block_id, block_hash)
        {
            self.blockring.add_block(&block);
        }
        //
        // blocks are stored in a hashmap indexed by the block_hash. we expect all
        // all block_hashes to be unique, so simply insert blocks one-by-one on
        // arrival if they do not exist.

        if !self.blocks.contains_key(&block_hash) {
            self.blocks.insert(block_hash, block);
        } else {
            error!(
                "BLOCK IS ALREADY IN THE BLOCKCHAIN, WHY ARE WE ADDING IT????? {:?}",
                block.get_hash()
            );
        }

        //
        // find shared ancestor of new_block with old_chain
        //
        let mut new_chain: Vec<[u8; 32]> = Vec::new();
        let mut old_chain: Vec<[u8; 32]> = Vec::new();
        let mut shared_ancestor_found = false;
        let mut new_chain_hash = block_hash;
        let mut old_chain_hash = previous_block_hash;

        while !shared_ancestor_found {
            if self.blocks.contains_key(&new_chain_hash) {
                if self.blocks.get(&new_chain_hash).unwrap().get_lc() {
                    shared_ancestor_found = true;
                    break;
                } else {
                    if new_chain_hash == [0; 32] {
                        break;
                    }
                }
                new_chain.push(new_chain_hash);
                new_chain_hash = self
                    .blocks
                    .get(&new_chain_hash)
                    .unwrap()
                    .get_previous_block_hash();
            } else {
                break;
            }
        }

        //
        // and get existing current chain for comparison
        //
        if shared_ancestor_found {
            loop {
                if new_chain_hash == old_chain_hash {
                    break;
                }
                if self.blocks.contains_key(&old_chain_hash) {
                    old_chain.push(old_chain_hash);
                    old_chain_hash = self
                        .blocks
                        .get(&old_chain_hash)
                        .unwrap()
                        .get_previous_block_hash();
                    if old_chain_hash == [0; 32] {
                        break;
                    }
                    if new_chain_hash == old_chain_hash {
                        break;
                    }
                }
            }
        } else {
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
                //
                error!("blocks received out-of-order issue...");
            }
        }

        //
        // at this point we should have a shared ancestor or not
        //
        // find out whether this new block is claiming to require chain-validation
        //
        let am_i_the_longest_chain = self.is_new_chain_the_longest_chain(&new_chain, &old_chain);

        //
        // validate
        //
        // blockchain validate "validates" the new_chain by unwinding the old
        // and winding the new, which calling validate on any new previously-
        // unvalidated blocks. When the longest-chain status of blocks changes
        // the function on_chain_reorganization is triggered in blocks and
        // with the BlockRing. We fail if the newly-preferred chain is not
        // viable.
        //
        if am_i_the_longest_chain {
            let does_new_chain_validate = self.validate(new_chain, old_chain, io_handler).await;
            if does_new_chain_validate {
                self.add_block_success(block_hash, io_handler).await;

                //
                // TODO
                //
                // mutable update is hell -- we can do this but have to have
                // manually checked that the entry exists in order to pull
                // this trick. we did this check before validating.
                //
                {
                    self.blocks.get_mut(&block_hash).unwrap().set_lc(true);
                }

                // TODO : send with the right channel
                // global_sender
                //     .send(GlobalEvent::BlockchainAddBlockSuccess { hash: block_hash })
                //     .expect("error: BlockchainAddBlockSuccess message failed to send");

                let difficulty = self.blocks.get(&block_hash).unwrap().get_difficulty();

                sender_to_miner
                    .send(MinerEvent::Mine {
                        hash: block_hash,
                        difficulty,
                    })
                    .await
                    .unwrap();
                debug!("event sent to miner");
                // global_sender
                //     .send(GlobalEvent::BlockchainNewLongestChainBlock {
                //         hash: block_hash,
                //         difficulty,
                //     })
                //     .expect("error: BlockchainNewLongestChainBlock message failed to send");
            } else {
                self.add_block_failure().await;

                // TODO : send with related channel
                // global_sender
                //     .send(GlobalEvent::BlockchainAddBlockFailure { hash: block_hash })
                //     .expect("error: BlockchainAddBlockFailure message failed to send");
            }
        } else {
            self.add_block_failure().await;

            // TODO : send with related channel
            // global_sender
            //     .send(GlobalEvent::BlockchainAddBlockFailure { hash: block_hash })
            //     .expect("error: BlockchainAddBlockFailure message failed to send");
        }
    }
    // pub async fn add_block_to_blockchain(blockchain_lock: Arc<RwLock<Blockchain>>, block: Block) {
    //     let mut blockchain = blockchain_lock.write().await;
    //     let res = blockchain.add_block(block).await;
    //     res
    // }

    pub async fn add_block_success(
        &mut self,
        block_hash: SaitoHash,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        debug!("add_block_success : {:?}", hex::encode(block_hash));
        // trace!(
        //     " ... blockchain.add_block_success: {:?}",
        //     create_timestamp()
        // );
        // print blockring longest_chain_block_hash infor
        self.print();

        //
        // save to disk
        //
        {
            let block = self.get_mut_block(&block_hash).await;
            if block.get_block_type() != BlockType::Header {
                Storage::write_block_to_disk(block, io_handler).await;
            }
        }

        //
        // TODO: clean up mempool - I think we shouldn't cleanup mempool here.
        //  because that's already happening in send_blocks_to_blockchain
        //  So who is in charge here?
        //  is send_blocks_to_blockchain calling add_block or
        //  is blockchain calling mempool.on_chain_reorganization?
        //
        //
        // mempool.delete_transactions(&block.get_transactions());

        //
        // propagate block to network
        //
        // TODO : notify other threads and propagate to other peers
        // global_sender
        //     .send(GlobalEvent::BlockchainSavedBlock { hash: block_hash })
        //     .expect("error: BlockchainSavedBlock message failed to send");
        // trace!(" ... block save done:            {:?}", create_timestamp());

        //
        // update_genesis_period and prune old data - MOVED to on_chain_reorganization()
        //
        // self.update_genesis_period().await;

        //
        // fork id  - MOVED to on_chain_reorganization()
        //
        // let fork_id = self.generate_fork_id(block_id);
        // self.set_fork_id(fork_id);

        //
        // ensure pruning of next block OK will have the right CVs
        //
        if self.get_latest_block_id() > GENESIS_PERIOD {
            let pruned_block_hash = self.blockring.get_longest_chain_block_hash_by_block_id(
                self.get_latest_block_id() - GENESIS_PERIOD,
            );

            //
            // TODO
            //
            // handle this more efficiently - we should be able to prepare the block
            // in advance so that this doesn't take up time in block production. we
            // need to generate_metadata_hashes so that the slips know the utxo_key
            // to use to check the utxoset.
            //
            {
                let pblock = self.get_mut_block(&pruned_block_hash).await;
                pblock
                    .upgrade_block_to_block_type(BlockType::Full, io_handler)
                    .await;
            }
        }
    }

    pub async fn add_block_failure(&mut self) {}

    pub fn generate_fork_id(&self, block_id: u64) -> SaitoHash {
        let mut fork_id = [0; 32];
        let mut current_block_id = block_id;

        //
        // roll back to last even 10 blocks
        //
        for i in 0..10 {
            if (current_block_id - i) % 10 == 0 {
                current_block_id -= i;
                break;
            }
        }

        //
        // loop backwards through blockchain
        //
        for i in 0..16 {
            if i == 0 {
                current_block_id -= 0;
            }
            if i == 1 {
                current_block_id -= 10;
            }
            if i == 2 {
                current_block_id -= 10;
            }
            if i == 3 {
                current_block_id -= 10;
            }
            if i == 4 {
                current_block_id -= 10;
            }
            if i == 5 {
                current_block_id -= 10;
            }
            if i == 6 {
                current_block_id -= 25;
            }
            if i == 7 {
                current_block_id -= 25;
            }
            if i == 8 {
                current_block_id -= 100;
            }
            if i == 9 {
                current_block_id -= 300;
            }
            if i == 10 {
                current_block_id -= 500;
            }
            if i == 11 {
                current_block_id -= 4000;
            }
            if i == 12 {
                current_block_id -= 10000;
            }
            if i == 13 {
                current_block_id -= 20000;
            }
            if i == 14 {
                current_block_id -= 50000;
            }
            if i == 15 {
                current_block_id -= 100000;
            }

            //
            // do not loop around if block id < 0
            //
            if current_block_id > block_id || current_block_id == 0 {
                break;
            }

            //
            // index to update
            //
            let idx = 2 * i;

            //
            //
            //
            let block_hash = self
                .blockring
                .get_longest_chain_block_hash_by_block_id(current_block_id);
            fork_id[idx] = block_hash[idx];
            fork_id[idx + 1] = block_hash[idx + 1];
        }

        fork_id
    }

    pub fn generate_last_shared_ancestor(
        &self,
        peer_latest_block_id: u64,
        fork_id: SaitoHash,
    ) -> u64 {
        let my_latest_block_id = self.get_latest_block_id();

        let mut pbid = peer_latest_block_id;
        let mut mbid = my_latest_block_id;

        if peer_latest_block_id >= my_latest_block_id {
            //
            // roll back to last even 10 blocks
            //
            for i in 0..10 {
                if (pbid - i) % 10 == 0 {
                    pbid -= i;
                    break;
                }
            }

            //
            // their fork id
            //
            for i in 0..16 {
                if i == 0 {
                    pbid -= 0;
                }
                if i == 1 {
                    pbid -= 10;
                }
                if i == 2 {
                    pbid -= 10;
                }
                if i == 3 {
                    pbid -= 10;
                }
                if i == 4 {
                    pbid -= 10;
                }
                if i == 5 {
                    pbid -= 10;
                }
                if i == 6 {
                    pbid -= 25;
                }
                if i == 7 {
                    pbid -= 25;
                }
                if i == 8 {
                    pbid -= 100;
                }
                if i == 9 {
                    pbid -= 300;
                }
                if i == 10 {
                    pbid -= 500;
                }
                if i == 11 {
                    pbid -= 4000;
                }
                if i == 12 {
                    pbid -= 10000;
                }
                if i == 13 {
                    pbid -= 20000;
                }
                if i == 14 {
                    pbid -= 50000;
                }
                if i == 15 {
                    pbid -= 100000;
                }

                //
                // do not loop around if block id < 0
                //
                if pbid > peer_latest_block_id || pbid == 0 {
                    return 0;
                }

                //
                // index in fork_id hash
                //
                let idx = 2 * i;

                //
                // compare input hash to my hash
                //
                if pbid <= mbid {
                    let block_hash = self
                        .blockring
                        .get_longest_chain_block_hash_by_block_id(pbid);
                    if fork_id[idx] == block_hash[idx] && fork_id[idx + 1] == block_hash[idx + 1] {
                        return pbid;
                    }
                }
            }
        } else {
            //
            // their fork id
            //
            for i in 0..16 {
                if i == 0 {
                    mbid -= 0;
                }
                if i == 1 {
                    mbid -= 10;
                }
                if i == 2 {
                    mbid -= 10;
                }
                if i == 3 {
                    mbid -= 10;
                }
                if i == 4 {
                    mbid -= 10;
                }
                if i == 5 {
                    mbid -= 10;
                }
                if i == 6 {
                    mbid -= 25;
                }
                if i == 7 {
                    mbid -= 25;
                }
                if i == 8 {
                    mbid -= 100;
                }
                if i == 9 {
                    mbid -= 300;
                }
                if i == 10 {
                    mbid -= 500;
                }
                if i == 11 {
                    mbid -= 4000;
                }
                if i == 12 {
                    mbid -= 10000;
                }
                if i == 13 {
                    mbid -= 20000;
                }
                if i == 14 {
                    mbid -= 50000;
                }
                if i == 15 {
                    mbid -= 100000;
                }

                //
                // do not loop around if block id < 0
                //
                if mbid > my_latest_block_id || mbid == 0 {
                    return 0;
                }

                //
                // index in fork_id hash
                //
                let idx = 2 * i;

                //
                // compare input hash to my hash
                //
                if pbid <= mbid {
                    let block_hash = self
                        .blockring
                        .get_longest_chain_block_hash_by_block_id(pbid);
                    if fork_id[idx] == block_hash[idx] && fork_id[idx + 1] == block_hash[idx + 1] {
                        return pbid;
                    }
                }
            }
        }

        //
        // no match? return 0 -- no shared ancestor
        //
        0
    }
    pub fn print(&self) {
        let latest_block_id = self.get_latest_block_id();
        let mut current_id = latest_block_id;

        while current_id > 0 {
            info!(
                "{} - {:?}",
                current_id,
                hex::encode(
                    self.blockring
                        .get_longest_chain_block_hash_by_block_id(current_id)
                )
            );
            current_id -= 1;
        }
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
    pub async fn get_block(&self, block_hash: &SaitoHash) -> Option<&Block> {
        // TODO : load from disk if not found
        self.blocks.get(block_hash)
    }

    pub async fn get_mut_block(&mut self, block_hash: &SaitoHash) -> &mut Block {
        let block = self.blocks.get_mut(block_hash).unwrap();
        block
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
        new_chain: &Vec<[u8; 32]>,
        old_chain: &Vec<[u8; 32]>,
    ) -> bool {
        debug!("checking for longest chain");
        if old_chain.len() > new_chain.len() {
            warn!("WARN: old chain length is greater than new chain length");
            return false;
        }

        if self.blockring.get_latest_block_id() >= self.blocks.get(&new_chain[0]).unwrap().get_id()
        {
            return false;
        }

        let mut old_bf: u64 = 0;
        let mut new_bf: u64 = 0;

        for hash in old_chain.iter() {
            old_bf += self.blocks.get(hash).unwrap().get_burnfee();
        }
        for hash in new_chain.iter() {
            new_bf += self.blocks.get(hash).unwrap().get_burnfee();
        }
        //
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
        new_chain: Vec<[u8; 32]>,
        old_chain: Vec<[u8; 32]>,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> bool {
        debug!("validating chains");
        //
        // ensure new chain has adequate mining support to be considered as
        // a viable chain. we handle this check here as opposed to handling
        // it in wind_chain as we only need to check once for the entire chain
        //
        let mut golden_tickets_found = 0;
        let mut search_depth_idx = 0;
        let mut latest_block_hash = new_chain[0];

        for i in 0..MIN_GOLDEN_TICKETS_DENOMINATOR {
            search_depth_idx += 1;

            if let Some(block) = self.get_block_sync(&latest_block_hash) {
                if i == 0 {
                    if block.get_id() < MIN_GOLDEN_TICKETS_DENOMINATOR {
                        break;
                    }
                }

                // the latest block will not have has_golden_ticket set yet
                // so it is possible we undercount the latest block. this
                // is dealt with by manually checking for the existence of
                // a golden ticket if we only have 1 golden ticket below.
                if block.get_has_golden_ticket() {
                    golden_tickets_found += 1;
                }
                latest_block_hash = block.get_previous_block_hash();
            } else {
                break;
            }
        }

        if golden_tickets_found < MIN_GOLDEN_TICKETS_NUMERATOR
            && search_depth_idx >= MIN_GOLDEN_TICKETS_DENOMINATOR
        {
            let mut return_value = false;
            if let Some(block) = self.get_block_sync(&new_chain[0]) {
                for transaction in block.get_transactions() {
                    if transaction.get_transaction_type() == TransactionType::GoldenTicket {
                        return_value = true;
                        break;
                    }
                }
            }
            if !return_value {
                return false;
            }
        }

        if !old_chain.is_empty() {
            let res = self
                .unwind_chain(&new_chain, &old_chain, 0, true, io_handler)
                //.unwind_chain(&new_chain, &old_chain, old_chain.len() - 1, true)
                .await;
            res
        } else if !new_chain.is_empty() {
            let res = self
                .wind_chain(
                    &new_chain,
                    &old_chain,
                    new_chain.len() - 1,
                    false,
                    io_handler,
                )
                .await;
            res
        } else {
            true
        }
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
        new_chain: &Vec<[u8; 32]>,
        old_chain: &Vec<[u8; 32]>,
        current_wind_index: usize,
        wind_failure: bool,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> bool {
        // trace!(" ... blockchain.wind_chain strt: {:?}", create_timestamp());

        //
        // if we are winding a non-existent chain with a wind_failure it
        // means our wind attempt failed and we should move directly into
        // add_block_failure() by returning false.
        //
        if wind_failure && new_chain.is_empty() {
            return false;
        }

        //
        // winding the chain requires us to have certain data associated
        // with the block and the transactions, particularly the tx hashes
        // that we need to generate the slip UUIDs and create the tx sigs.
        //
        // we fetch the block mutably first in order to update these vars.
        // we cannot just send the block mutably into our regular validate()
        // function because of limitatins imposed by Rust on mutable data
        // structures. So validation is "read-only" and our "write" actions
        // happen first.
        //
        {
            let mut block = self.get_mut_block(&new_chain[current_wind_index]).await;
            block.generate_metadata();

            let latest_block_id = block.get_id();

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
                    self.blockring.get_longest_chain_block_hash_by_block_id(bid);
                if self.is_block_indexed(previous_block_hash) {
                    block = self.get_mut_block(&previous_block_hash).await;
                    block
                        .upgrade_block_to_block_type(BlockType::Full, io_handler)
                        .await;
                }
            }
        }

        let block = self.blocks.get(&new_chain[current_wind_index]).unwrap();
        // trace!(" ... before block.validate:      {:?}", create_timestamp());
        let does_block_validate = block.validate(&self, &self.utxoset, &self.staking).await;

        // trace!(
        //     " ... after block.validate:       {:?} {}",
        //     create_timestamp(),
        //     does_block_validate
        // );

        if does_block_validate {
            // trace!(" ... before block ocr            {:?}", create_timestamp());

            // utxoset update
            block.on_chain_reorganization(&mut self.utxoset, true);

            // trace!(" ... before blockring ocr:       {:?}", create_timestamp());

            // blockring update
            self.blockring
                .on_chain_reorganization(block.get_id(), block.get_hash(), true);

            // staking tables update
            let (res_spend, res_unspend, res_delete) =
                self.staking.on_chain_reorganization(block, true);

            //
            // TODO - wallet update should be optional, as core routing nodes
            // will not want to do the work of scrolling through the block and
            // updating their wallets by default. wallet processing can be
            // more efficiently handled by lite-nodes.
            //
            {
                // trace!(" ... wallet processing start:    {}", create_timestamp());

                trace!("waiting for the wallet write lock");
                let mut wallet = self.wallet_lock.write().await;
                trace!("acquired the wallet write lock");
                wallet.on_chain_reorganization(&block, true);

                // trace!(" ... wallet processing stop:     {}", create_timestamp());
            }

            let block_id = block.get_id();
            self.on_chain_reorganization(block_id, true, io_handler)
                .await;

            //
            // we cannot pass the UTXOSet into the staking object to update as that would
            // require multiple mutable borrows of the blockchain object, so we receive
            // return vectors of the slips that need to be inserted, spent or deleted and
            // handle this after-the-fact. this keeps the UTXOSet up-to-date with whatever
            // is in the staking tables.
            //
            for i in 0..res_spend.len() {
                res_spend[i].on_chain_reorganization(&mut self.utxoset, true, 1);
            }
            for i in 0..res_unspend.len() {
                res_spend[i].on_chain_reorganization(&mut self.utxoset, true, 0);
            }
            for i in 0..res_delete.len() {
                res_spend[i].delete(&mut self.utxoset);
            }

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
                    io_handler,
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
            error!("ERROR: this block does not validate!");
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
                if old_chain.len() > 0 {
                    info!("old chain len: {}", old_chain.len());
                    let res = self
                        .wind_chain(old_chain, new_chain, old_chain.len() - 1, true, io_handler)
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
                    chain_to_unwind.push(new_chain[i].clone());
                }

                //
                // chain to unwind is now something like this...
                //
                //  [3] [2] [1]
                //
                // unwinding starts from the BEGINNING of the vector
                //
                let res = self
                    .unwind_chain(old_chain, &chain_to_unwind, 0, true, io_handler)
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
        new_chain: &Vec<[u8; 32]>,
        old_chain: &Vec<[u8; 32]>,
        current_unwind_index: usize,
        wind_failure: bool,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) -> bool {
        let block = &self.blocks[&old_chain[current_unwind_index]];

        // utxoset update
        block.on_chain_reorganization(&mut self.utxoset, false);

        // blockring update
        self.blockring
            .on_chain_reorganization(block.get_id(), block.get_hash(), false);

        // staking tables
        let (res_spend, res_unspend, res_delete) =
            self.staking.on_chain_reorganization(block, false);

        // wallet update
        {
            trace!("waiting for the wallet write lock");
            let mut wallet = self.wallet_lock.write().await;
            trace!("acquired the wallet write lock");
            wallet.on_chain_reorganization(&block, false);
        }

        //
        // we cannot pass the UTXOSet into the staking object to update as that would
        // require multiple mutable borrows of the blockchain object, so we receive
        // return vectors of the slips that need to be inserted, spent or deleted and
        // handle this after-the-fact. this keeps the UTXOSet up-to-date with whatever
        // is in the staking tables.
        //
        for i in 0..res_spend.len() {
            res_spend[i].on_chain_reorganization(&mut self.utxoset, true, 1);
        }
        for i in 0..res_unspend.len() {
            res_spend[i].on_chain_reorganization(&mut self.utxoset, true, 0);
        }
        for i in 0..res_delete.len() {
            res_spend[i].delete(&mut self.utxoset);
        }

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
                    io_handler,
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
                    io_handler,
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
        longest_chain: bool,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        //
        // skip out if earlier than we need to be vis-a-vis last_block_id
        //
        if self.get_latest_block_id() >= block_id {
            return;
        }

        if longest_chain {
            //
            // update genesis period, purge old data
            //
            self.update_genesis_period(io_handler).await;

            //
            // generate fork_id
            //
            let fork_id = self.generate_fork_id(block_id);
            self.set_fork_id(fork_id);
        }

        self.downgrade_blockchain_data().await;
    }

    pub async fn update_genesis_period(
        &mut self,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        //
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

            //
            // in either case, we are OK to throw out everything below the
            // lowest_block_id that we have found. we use the purge_id to
            // handle purges.
            //
            self.delete_blocks(purge_bid, io_handler).await;
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
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        trace!(
            "removing data including from disk at id {}",
            delete_block_id
        );

        let mut block_hashes_copy: Vec<SaitoHash> = vec![];

        {
            let block_hashes = self.blockring.get_block_hashes_at_block_id(delete_block_id);
            for hash in block_hashes {
                block_hashes_copy.push(hash.clone());
            }
        }

        trace!("number of hashes to remove {}", block_hashes_copy.len());

        for hash in block_hashes_copy {
            self.delete_block(delete_block_id, hash, io_handler).await;
        }
    }

    //
    // deletes a single block
    //
    pub async fn delete_block(
        &mut self,
        delete_block_id: u64,
        delete_block_hash: SaitoHash,
        io_handler: &mut Box<dyn HandleIo + Send + Sync>,
    ) {
        //
        // ask block to delete itself / utxo-wise
        //
        {
            let pblock = self.blocks.get(&delete_block_hash).unwrap();
            let pblock_filename = Storage::generate_block_filename(pblock, io_handler);

            //
            // remove slips from wallet
            //
            {
                trace!("waiting for the wallet write lock");
                let mut wallet = self.wallet_lock.write().await;
                trace!("acquired the wallet write lock");
                wallet.delete_block(pblock);
            }
            //
            // removes utxoset data
            //
            pblock.delete(&mut self.utxoset).await;

            //
            // deletes block from disk
            //
            Storage::delete_block_from_disk(pblock_filename, io_handler).await;
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

    pub async fn downgrade_blockchain_data(&mut self) {
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
                block_hashes_copy.push(hash.clone());
            }
        }

        for hash in block_hashes_copy {
            //
            // ask the block to remove its transactions
            //
            {
                let pblock = self.get_mut_block(&hash).await;
                pblock
                    .downgrade_block_to_block_type(BlockType::Pruned)
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::core::data::blockchain::{bit_pack, bit_unpack, Blockchain};
    use crate::core::data::wallet::Wallet;

    #[tokio::test]
    async fn test_blockchain_init() {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
        let blockchain = Blockchain::new(wallet);

        assert_eq!(blockchain.fork_id, [0; 32]);
        assert_eq!(blockchain.genesis_block_id, 0);
    }

    #[tokio::test]
    async fn test_add_block() {
        let wallet = Arc::new(RwLock::new(Wallet::new()));
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
}

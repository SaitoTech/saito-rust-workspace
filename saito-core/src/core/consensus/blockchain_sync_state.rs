use std::cmp::min;
use std::collections::VecDeque;

use ahash::HashMap;
use log::trace;

use crate::core::defs::{BlockId, PeerIndex, PrintForLog, SaitoHash};

#[derive(Debug)]
enum BlockStatus {
    Queued,
    Fetching,
    Fetched,
}
/// Maintains the state for fetching blocks from other peers into this peer.
/// Tries to fetch the blocks in the most resource efficient way possible.
pub struct BlockchainSyncState {
    /// These are the blocks we have received from each of our peers
    received_block_picture: HashMap<PeerIndex, VecDeque<(BlockId, SaitoHash)>>,
    /// These are the blocks which we have to fetch from each of our peers
    blocks_to_fetch: HashMap<PeerIndex, VecDeque<(SaitoHash, BlockStatus, BlockId)>>,
    /// We only fetch within a certain window to make sure we process what we receive and to reduce the burden of the fetched peers
    block_fetch_ceiling: BlockId,
    batch_size: usize,
}

impl BlockchainSyncState {
    pub fn new(batch_size: usize) -> BlockchainSyncState {
        BlockchainSyncState {
            received_block_picture: Default::default(),
            blocks_to_fetch: Default::default(),
            block_fetch_ceiling: batch_size as BlockId,
            batch_size,
        }
    }

    /// Builds the list of blocks to be fetched from each peer. Blocks fetched are in order if in the same fork,
    /// or at the same level for multiple forks to make sure the blocks fetched can be processed most efficiently
    pub(crate) fn build_peer_block_picture(&mut self) {
        trace!("building peer block picture");
        // for every block picture received from a peer, we sort and create a list of sequential hashes to fetch from peers
        for (peer_index, received_picture_from_peer) in self.received_block_picture.iter_mut() {
            if received_picture_from_peer.is_empty() {
                continue;
            }

            // need to sort before sequencing
            received_picture_from_peer.make_contiguous().sort_by(
                |(id_a, hash_a), (id_b, hash_b)| {
                    if id_a == id_b {
                        return hash_a.cmp(hash_b);
                    }
                    id_a.cmp(id_b)
                },
            );

            let result = self.blocks_to_fetch.contains_key(peer_index);
            if !result {
                // if we don't have an entry, we find the minimum block id for the peer which is the first (since we sorted)
                let (id, hash) = received_picture_from_peer
                    .pop_front()
                    .expect("received picture should not be empty");
                trace!(
                    "adding new block hash : {:?} for peer : {:?} since nothing found",
                    hash.to_hex(),
                    peer_index
                );
                let mut deq = VecDeque::new();
                deq.push_back((hash, BlockStatus::Queued, id));
                self.blocks_to_fetch.insert(*peer_index, deq);
            }
            loop {
                if received_picture_from_peer.is_empty() {
                    break;
                }
                let result = self.blocks_to_fetch.get_mut(peer_index);
                if result.is_none() {
                    break;
                }
                let blocks_to_fetch_from_peer = result.expect("fetching list should exist");

                if blocks_to_fetch_from_peer.is_empty() {
                    break;
                }
                let (_, _, max_block_id_to_fetch) = blocks_to_fetch_from_peer
                    .back()
                    .expect("failed getting back item from fetching list");
                if received_picture_from_peer.is_empty() {
                    break;
                }
                let (min_received_block_id, _) = received_picture_from_peer
                    .front()
                    .expect("failed getting front item from received picture");
                trace!(
                    "for peer : {:?} last at fetching list : {:?}, first at received list : {:?}",
                    peer_index,
                    max_block_id_to_fetch,
                    min_received_block_id
                );
                if *max_block_id_to_fetch != *min_received_block_id
                    && *max_block_id_to_fetch + 1 != *min_received_block_id
                {
                    // if first entry in the received pic is not next in sequence (either has next block id or same block id for forks) we break
                    break;
                }
                let (id, hash) = received_picture_from_peer
                    .pop_front()
                    .expect("failed popping front from received picture");
                blocks_to_fetch_from_peer.push_back((hash, BlockStatus::Queued, id));
            }
        }
        // removing empty lists from memory
        self.received_block_picture.retain(|_, map| !map.is_empty());
        self.blocks_to_fetch.retain(|_, vec| !vec.is_empty());
    }

    /// Generates the list of blocks which needs to be fetched next. A list is generated per each block since we can fetch from multiple peers concurrently.
    pub fn get_block_list_to_fetch_per_peer(
        &mut self,
    ) -> HashMap<PeerIndex, Vec<(SaitoHash, BlockId)>> {
        trace!(
            "getting block list to fetch for each peer. block fetch ceiling : {:?}",
            self.block_fetch_ceiling
        );
        let mut selected_blocks_per_peer: HashMap<u64, Vec<(SaitoHash, BlockId)>> =
            Default::default();

        // for each peer check if we can fetch block
        for (peer_index, block_metadata) in self.blocks_to_fetch.iter_mut() {
            // check if we have blocks to fetch within our batch size
            for i in 0..min(block_metadata.len(), self.batch_size) {
                // TODO : same block can be fetched from multiple peers as of now. need to define the expected behaviour
                let (hash, status, block_id) = block_metadata
                    .get_mut(i)
                    .expect("entry should exist since we are checking the length");
                if *block_id > self.block_fetch_ceiling {
                    trace!(
                        "block : {:?} - {:?} is above the ceiling : {:?}",
                        block_id,
                        hash.to_hex(),
                        self.block_fetch_ceiling
                    );
                    break;
                }
                if let BlockStatus::Queued = status {
                    trace!(
                        "block : {:?} : {:?} to be fetched from peer : {:?}",
                        block_id,
                        hash.to_hex(),
                        peer_index
                    );
                    selected_blocks_per_peer
                        .entry(*peer_index)
                        .or_default()
                        .push((*hash, *block_id));
                } else {
                    trace!(
                        "block {:?} - {:?} status = {:?}",
                        block_id,
                        hash.to_hex(),
                        status
                    );
                }
            }
            trace!(
                "peer : {:?} to be fetched {:?} blocks. first : {:?} last : {:?}",
                peer_index,
                block_metadata.len(),
                block_metadata.front().unwrap().2,
                block_metadata.back().unwrap().2
            );
        }

        selected_blocks_per_peer
    }
    /// Mark each of the blocks in the list as "fetching"
    ///
    /// # Arguments
    ///
    /// * `entries`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn mark_as_fetching(&mut self, entries: Vec<(PeerIndex, SaitoHash)>) {
        trace!("marking as fetching : {:?}", entries.len());
        for (peer_index, hash) in entries.iter() {
            let res = self.blocks_to_fetch.get_mut(peer_index);
            if res.is_none() {
                continue;
            }
            let res = res.unwrap();
            for (block_hash, status, _) in res {
                if hash.eq(block_hash) {
                    *status = BlockStatus::Fetching;
                    trace!("block : {:?} marked as fetching", block_hash.to_hex());
                    break;
                }
            }
        }
    }
    /// Mark the block state as "fetched"
    ///
    /// # Arguments
    ///
    /// * `peer_index`:
    /// * `hash`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn mark_as_fetched(&mut self, peer_index: PeerIndex, hash: SaitoHash) {
        let res = self.blocks_to_fetch.get_mut(&peer_index);
        if res.is_none() {
            trace!(
                "block : {:?} for peer : {:?} not found to mark as fetched",
                hash.to_hex(),
                peer_index
            );
            return;
        }
        let res = res.unwrap();
        for (block_hash, status, _) in res {
            if hash.eq(block_hash) {
                *status = BlockStatus::Fetched;
                trace!(
                    "block : {:?} marked as fetched from peer : {:?}",
                    block_hash.to_hex(),
                    peer_index
                );
                break;
            }
        }
        self.clean_fetched(peer_index);
    }

    /// Removes all the entries related to fetched blocks and removes any empty collections from memory
    ///
    /// # Arguments
    ///
    /// * `peer_index`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    fn clean_fetched(&mut self, peer_index: PeerIndex) {
        trace!("cleaning fetched : {:?}", peer_index);
        if let Some(res) = self.blocks_to_fetch.get_mut(&peer_index) {
            while let Some((hash, status, id)) = res.front() {
                match status {
                    BlockStatus::Fetched => {}
                    _ => {
                        // since the list is ordered, we can break the loop at the first not(Fetched) result
                        break;
                    }
                }
                trace!(
                    "removing hash : {:?} - {:?} from peer : {:?}",
                    hash.to_hex(),
                    id,
                    peer_index
                );
                res.pop_front();
            }
        }
        self.blocks_to_fetch.retain(|_, map| !map.is_empty());
    }
    /// Adds an entry to this data structure which will be fetched later after prioritizing.
    ///
    /// # Arguments
    ///
    /// * `block_hash`:
    /// * `block_id`:
    /// * `peer_index`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn add_entry(&mut self, block_hash: SaitoHash, block_id: BlockId, peer_index: PeerIndex) {
        trace!(
            "add entry : {:?} - {:?} from {:?}",
            block_hash.to_hex(),
            block_id,
            peer_index
        );
        if self.blocks_to_fetch.is_empty() {
            // If the current list to fetch is empty, we don't have to fetch the next block (might have already fetched) but the next received block hash
            self.set_latest_blockchain_id(block_id);
        }
        self.received_block_picture
            .entry(peer_index)
            .or_default()
            .push_back((block_id, block_hash));
    }
    /// Removes entry when the hash is added to the blockchain. If so we can move the block ceiling up.
    ///
    /// # Arguments
    ///
    /// * `block_hash`:
    /// * `peer_index`:
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn remove_entry(&mut self, block_hash: SaitoHash, peer_index: PeerIndex) {
        trace!(
            "removing entry : {:?} from peer : {:?}",
            block_hash.to_hex(),
            peer_index
        );
        if let Some(hashes) = self.blocks_to_fetch.get_mut(&peer_index) {
            hashes.retain(|(hash, _, _)| !block_hash.eq(hash));
        }
        self.blocks_to_fetch.retain(|_, map| !map.is_empty());
    }
    pub fn get_stats(&self) -> Vec<String> {
        let mut stats = vec![];
        for (peer_index, vec) in self.blocks_to_fetch.iter() {
            let res = self.received_block_picture.get(peer_index);
            let mut count = 0;
            if let Some(deq) = res {
                count = deq.len();
            }
            let mut highest_id = 0;
            let last = vec.back();
            if let Some((_, _, id)) = last {
                highest_id = *id;
            }
            let mut lowest_id = 0;
            let first = vec.front();
            if first.is_some() {
                lowest_id = first.unwrap().2;
            }
            let fetching_blocks_count = vec
                .iter()
                .filter(|(_, status, _)| matches!(status, BlockStatus::Fetching))
                .count();
            let stat = format!(
                "{} - peer : {:?} lowest_id: {:?} fetching_count : {:?} ordered_till : {:?} unordered_block_ids : {:?}",
                format!("{:width$}", "routing::sync_state", width = 40),
                peer_index,
                lowest_id,
                fetching_blocks_count,
                highest_id,
                count
            );
            stats.push(stat);
        }
        let stat = format!(
            "{} - block_fetch_ceiling : {:?}",
            format!("{:width$}", "routing::sync_state", width = 40),
            self.block_fetch_ceiling
        );
        stats.push(stat);
        stats
    }
    pub fn set_latest_blockchain_id(&mut self, id: BlockId) {
        // TODO : batch size should be larger than the fork length diff which can change the current fork.
        // otherwise we won't fetch the blocks for new longest fork until current fork adds new blocks
        self.block_fetch_ceiling = id + self.batch_size as BlockId;
        trace!(
            "setting latest blockchain id : {:?} and ceiling : {:?}",
            id,
            self.block_fetch_ceiling
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::core::consensus::blockchain_sync_state::BlockchainSyncState;
    use crate::core::defs::BlockId;

    #[test]
    fn single_peer_window_test() {
        let mut state = BlockchainSyncState::new(20);

        for i in 0..state.batch_size + 2 {
            state.add_entry([(i + 1) as u8; 32], (i + 1) as u64, 1);
        }
        state.add_entry([200; 32], 200, 1);
        state.add_entry([201; 32], 201, 1);

        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        for i in 0..state.batch_size {
            let (entry, _) = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
        }
        let vec = vec![(1, [2; 32]), (1, [5; 32])];
        state.mark_as_fetching(vec);
        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size - 2);

        state.remove_entry([2; 32], 1);
        state.remove_entry([5; 32], 1);
        state.remove_entry([8; 32], 1);

        state.build_peer_block_picture();
        state.set_latest_blockchain_id(30);
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size - 1);
    }

    #[test]
    #[serial_test::serial]
    fn fetch_count_test() {
        // pretty_env_logger::init();
        let mut state = BlockchainSyncState::new(3);
        for i in 0..state.batch_size + 50 {
            state.add_entry([(i + 1) as u8; 32], (i + 1) as u64, 1);
        }
        state.add_entry([100; 32], 100, 1);
        state.add_entry([200; 32], 200, 1);

        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        assert_eq!(state.batch_size, 3);
        for i in 0..state.batch_size {
            let (entry, _) = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
        }
        let vec = vec![(1, [1; 32]), (1, [2; 32]), (1, [3; 32])];
        state.mark_as_fetching(vec);
        state.build_peer_block_picture();
        let result = state.get_block_list_to_fetch_per_peer();
        assert!(result.is_empty());
        state.remove_entry([1; 32], 1);
        state.remove_entry([3; 32], 1);
        state.build_peer_block_picture();
        let result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);

        state.set_latest_blockchain_id(1);
        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1).unwrap();
        assert_eq!(vec.len(), 1);

        state.remove_entry([2; 32], 1);
        state.set_latest_blockchain_id(3);
        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1).unwrap();
        assert_eq!(vec.len(), 3);
    }

    #[test]
    fn multiple_forks_from_multiple_peers_test() {
        let mut state = BlockchainSyncState::new(10);
        for i in 0..state.batch_size + 50 {
            state.add_entry([(i + 1) as u8; 32], (i + 1) as BlockId, 1);
        }
        for i in 4..state.batch_size + 50 {
            state.add_entry([(i + 101) as u8; 32], (i + 1) as BlockId, 1);
        }

        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        assert_eq!(state.batch_size, 10);
        let mut fetching = vec![];
        for i in 0..4 {
            let (entry, _) = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
            fetching.push((1, [(i + 1) as u8; 32]));
        }
        let mut value = 4;
        for index in (4..10).step_by(2) {
            value += 1;
            let (entry, _) = vec.get(index).unwrap();
            assert_eq!(*entry, [(value) as u8; 32]);
            fetching.push((1, [(value) as u8; 32]));

            let (entry, _) = vec.get(index + 1).unwrap();
            assert_eq!(*entry, [(value + 100) as u8; 32]);
            fetching.push((1, [(value + 100) as u8; 32]));
        }
        state.mark_as_fetching(fetching);
        state.build_peer_block_picture();
        let result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 0);

        state.remove_entry([1; 32], 1);
        state.remove_entry([5; 32], 1);
        state.remove_entry([106; 32], 1);
        state.build_peer_block_picture();
        let mut result = state.get_block_list_to_fetch_per_peer();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1).unwrap();
        assert_eq!(vec.len(), 3);
        // TODO : fix this
        // assert!(vec.contains(&[8; 32]));
        // assert!(vec.contains(&[108; 32]));
        // assert!(vec.contains(&[9; 32]));
    }
}

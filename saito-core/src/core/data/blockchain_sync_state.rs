use std::cmp::min;
use std::collections::VecDeque;

use ahash::HashMap;
use log::{debug, trace};

use crate::common::defs::{BlockId, PeerIndex, SaitoHash};

#[derive(Debug)]
enum BlockStatus {
    Queued,
    Fetching,
    Fetched,
}

pub struct BlockchainSyncState {
    received_block_picture: HashMap<PeerIndex, VecDeque<(BlockId, SaitoHash)>>,
    blocks_to_fetch: HashMap<PeerIndex, VecDeque<(SaitoHash, BlockStatus, BlockId)>>,
    /// since we are maintaining this state in routing thread and adding to blockchain in other thread, we need to keep a ceiling value for allowed block ids
    block_ceiling: BlockId,
    batch_size: usize,
}

impl BlockchainSyncState {
    pub fn new(batch_size: usize) -> BlockchainSyncState {
        BlockchainSyncState {
            received_block_picture: Default::default(),
            blocks_to_fetch: Default::default(),
            block_ceiling: batch_size as BlockId,
            batch_size,
        }
    }
    pub(crate) fn build_peer_block_picture(&mut self) {
        debug!("building peer block picture");
        // for every block picture received from a peer, we sort and create a list of sequential hashes to fetch from peers
        for (peer_index, received_picture) in self.received_block_picture.iter_mut() {
            if received_picture.is_empty() {
                continue;
            }

            // need to sort before sequencing
            received_picture
                .make_contiguous()
                .sort_by(|(id_a, hash_a), (id_b, hash_b)| {
                    if id_a == id_b {
                        return hash_a.cmp(hash_b);
                    }
                    id_a.cmp(id_b)
                });

            let result = self.blocks_to_fetch.contains_key(peer_index);
            if !result {
                // if we don't have an entry, we find the minimum block id for the peer which is the first (since we sorted)
                let (id, hash) = received_picture
                    .pop_front()
                    .expect("received picture should not be empty");
                debug!(
                    "adding new block hash : {:?} for peer : {:?} since nothing found",
                    hex::encode(hash),
                    peer_index
                );
                let mut deq = VecDeque::new();
                deq.push_back((hash, BlockStatus::Queued, id));
                self.blocks_to_fetch.insert(*peer_index, deq);
            }
            loop {
                if received_picture.is_empty() {
                    break;
                }
                let result = self.blocks_to_fetch.get_mut(peer_index);
                if result.is_none() {
                    break;
                }
                let fetching_list = result.unwrap();

                let (_, _, block_id) = fetching_list.back().unwrap();
                let (id, _) = received_picture.front().unwrap();
                trace!(
                    "for peer : {:?} last at fetching list : {:?}, first at received list : {:?}",
                    peer_index,
                    block_id,
                    id
                );
                if *block_id != *id && *block_id + 1 != *id {
                    // if first entry in the received pic is not next in sequence (either has next block id or same block id for forks) we break
                    break;
                }
                let (id, hash) = received_picture.pop_front().unwrap();
                fetching_list.push_back((hash, BlockStatus::Queued, id));
            }
        }
        self.received_block_picture.retain(|_, map| !map.is_empty());
        self.blocks_to_fetch.retain(|_, vec| !vec.is_empty());
    }

    pub fn request_blocks_from_waitlist(&mut self) -> HashMap<PeerIndex, Vec<SaitoHash>> {
        debug!("requesting blocks from waiting list");
        let mut result: HashMap<u64, Vec<SaitoHash>> = Default::default();

        // for each peer check if we can fetch block
        for (peer_index, hashes) in self.blocks_to_fetch.iter_mut() {
            // check if we have blocks to fetch within our batch size
            for i in 0..min(hashes.len(), self.batch_size) {
                // TODO : same block can be fetched from multiple peers as of now. need to define the expected behaviour
                let (hash, status, block_id) = hashes
                    .get_mut(i)
                    .expect("entry should exist since we are checking the length");
                if *block_id > self.block_ceiling {
                    debug!(
                        "block : {:?} - {:?} is above the ceiling : {:?}",
                        block_id,
                        hex::encode(hash),
                        self.block_ceiling
                    );
                    break;
                }
                if let BlockStatus::Queued = status {
                    debug!(
                        "block : {:?} : {:?} to be fetched from peer : {:?}",
                        block_id,
                        hex::encode(*hash),
                        peer_index
                    );
                    result.entry(*peer_index).or_default().push(*hash);
                } else {
                    debug!(
                        "block {:?} - {:?} status = {:?}",
                        block_id,
                        hex::encode(hash),
                        status
                    );
                }
            }
        }

        result
    }
    pub fn mark_as_fetching(&mut self, entries: Vec<(PeerIndex, SaitoHash)>) {
        debug!("marking as fetching : {:?}", entries.len());
        for (peer_index, hash) in entries.iter() {
            let res = self.blocks_to_fetch.get_mut(peer_index);
            if res.is_none() {
                continue;
            }
            let res = res.unwrap();
            for (block_hash, status, _) in res {
                if hash.eq(block_hash) {
                    *status = BlockStatus::Fetching;
                    debug!("block : {:?} marked as fetching", hex::encode(block_hash));
                    break;
                }
            }
        }
    }
    pub fn mark_as_fetched(&mut self, peer_index: PeerIndex, hash: SaitoHash) {
        let res = self.blocks_to_fetch.get_mut(&peer_index);
        if res.is_none() {
            debug!(
                "block : {:?} for peer : {:?} not found to mark as fetched",
                hex::encode(hash),
                peer_index
            );
            return;
        }
        let res = res.unwrap();
        for (block_hash, status, _) in res {
            if hash.eq(block_hash) {
                *status = BlockStatus::Fetched;
                debug!(
                    "block : {:?} marked as fetched from peer : {:?}",
                    hex::encode(block_hash),
                    peer_index
                );
                break;
            }
        }
        self.clean_fetched(peer_index);
    }
    fn clean_fetched(&mut self, peer_index: PeerIndex) {
        debug!("cleaning fetched : {:?}", peer_index);
        if let Some(res) = self.blocks_to_fetch.get_mut(&peer_index) {
            while let Some((hash, status, id)) = res.front() {
                if let BlockStatus::Fetched = status {
                } else {
                    break;
                }
                debug!(
                    "removing hash : {:?} - {:?} from peer : {:?}",
                    hex::encode(hash),
                    id,
                    peer_index
                );
                res.pop_front();
            }
        }
        self.blocks_to_fetch.retain(|_, map| !map.is_empty());
    }
    pub fn add_entry(&mut self, block_hash: SaitoHash, block_id: BlockId, peer_index: PeerIndex) {
        debug!(
            "add entry : {:?} - {:?} from {:?}",
            hex::encode(block_hash),
            block_id,
            peer_index
        );
        if self.blocks_to_fetch.is_empty() {
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
        debug!(
            "removing entry : {:?} from peer : {:?}",
            hex::encode(block_hash),
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
            let mut last_id = 0;
            let last = vec.back();
            if let Some((_, _, id)) = last {
                last_id = *id;
            }
            let mut first_id = 0;
            let first = vec.front();
            if first.is_some() {
                first_id = first.unwrap().2;
            }
            let fetching_count = vec
                .iter()
                .filter(|(_, status, _)| matches!(status, BlockStatus::Fetching))
                .count();
            let stat = format!(
                "{} - peer : {:?} first: {:?} fetching_count : {:?} ordered_till : {:?} waiting_to_order : {:?}",
                format!("{:width$}", "routing:sync_state", width = 40),
                peer_index,
                first_id,
                fetching_count,
                last_id,
                count
            );
            stats.push(stat);
        }
        let stat = format!(
            "{} - block_ceiling : {:?}",
            format!("{:width$}", "routing:sync_state", width = 40),
            self.block_ceiling
        );
        stats.push(stat);
        stats
    }
    pub fn set_latest_blockchain_id(&mut self, id: BlockId) {
        // TODO : batch size should be larger than the fork length diff which can change the current fork.
        // otherwise we won't fetch the blocks for new longest fork until current fork adds new blocks
        self.block_ceiling = id + self.batch_size as BlockId;
        debug!(
            "setting latest blockchain id : {:?} and ceiling : {:?}",
            id, self.block_ceiling
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::common::defs::BlockId;
    use crate::core::data::blockchain_sync_state::BlockchainSyncState;
    use log::info;

    #[test]
    fn single_peer_window_test() {
        let mut state = BlockchainSyncState::new(20);

        for i in 0..state.batch_size + 2 {
            state.add_entry([(i + 1) as u8; 32], (i + 1) as u64, 1);
        }
        state.add_entry([200; 32], 200, 1);
        state.add_entry([201; 32], 201, 1);

        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        for i in 0..state.batch_size {
            let entry = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
        }
        let vec = vec![(1, [2; 32]), (1, [5; 32])];
        state.mark_as_fetching(vec);
        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
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
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size - 1);
    }

    #[test]
    #[serial_test::serial]
    fn fetch_count_test() {
        pretty_env_logger::init();
        let mut state = BlockchainSyncState::new(3);
        for i in 0..state.batch_size + 50 {
            state.add_entry([(i + 1) as u8; 32], (i + 1) as u64, 1);
        }
        state.add_entry([100; 32], 100, 1);
        state.add_entry([200; 32], 200, 1);

        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        assert_eq!(state.batch_size, 3);
        for i in 0..state.batch_size {
            let entry = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
        }
        let vec = vec![(1, [1; 32]), (1, [2; 32]), (1, [3; 32])];
        state.mark_as_fetching(vec);
        state.build_peer_block_picture();
        let result = state.request_blocks_from_waitlist();
        assert!(result.is_empty());
        state.remove_entry([1; 32], 1);
        state.remove_entry([3; 32], 1);
        state.build_peer_block_picture();
        let result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);

        state.set_latest_blockchain_id(1);
        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1).unwrap();
        assert_eq!(vec.len(), 1);

        state.remove_entry([2; 32], 1);
        state.set_latest_blockchain_id(3);
        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
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
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1);
        assert!(vec.is_some());
        let vec = vec.unwrap();
        assert_eq!(vec.len(), state.batch_size);
        assert_eq!(state.batch_size, 10);
        let mut fetching = vec![];
        for i in 0..4 {
            let entry = vec.get(i).unwrap();
            assert_eq!(*entry, [(i + 1) as u8; 32]);
            fetching.push((1, [(i + 1) as u8; 32]));
        }
        let mut value = 4;
        for index in (4..10).step_by(2) {
            value += 1;
            let entry = vec.get(index).unwrap();
            assert_eq!(*entry, [(value) as u8; 32]);
            fetching.push((1, [(value) as u8; 32]));

            let entry = vec.get(index + 1).unwrap();
            assert_eq!(*entry, [(value + 100) as u8; 32]);
            fetching.push((1, [(value + 100) as u8; 32]));
        }
        state.mark_as_fetching(fetching);
        state.build_peer_block_picture();
        let result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 0);

        state.remove_entry([1; 32], 1);
        state.remove_entry([5; 32], 1);
        state.remove_entry([106; 32], 1);
        state.build_peer_block_picture();
        let mut result = state.request_blocks_from_waitlist();
        assert_eq!(result.len(), 1);
        let vec = result.get_mut(&1).unwrap();
        assert_eq!(vec.len(), 3);
        assert!(vec.contains(&[8; 32]));
        assert!(vec.contains(&[108; 32]));
        assert!(vec.contains(&[9; 32]));
    }
}

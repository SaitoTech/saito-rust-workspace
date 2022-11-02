use std::cmp::min;

use ahash::HashMap;
use tracing::debug;

use crate::common::defs::SaitoHash;

pub struct BlockchainSyncState {
    received_block_picture: HashMap<u64, HashMap<u64, SaitoHash>>,
    blocks_to_fetch: HashMap<u64, Vec<(SaitoHash, bool, u64)>>,
    /// since we are maintaining this state in routing thread and adding to blockchain in other thread, we need to keep a ceiling value for allowed block ids
    block_ceiling: u64,
    batch_size: usize,
}

impl BlockchainSyncState {
    pub fn new(batch_size: usize) -> BlockchainSyncState {
        BlockchainSyncState {
            received_block_picture: Default::default(),
            blocks_to_fetch: Default::default(),
            block_ceiling: batch_size as u64,
            batch_size,
        }
    }
    pub(crate) fn build_peer_block_picture(&mut self) {
        debug!("building peer block picture");
        for (peer_index, map) in self.received_block_picture.iter_mut() {
            if map.is_empty() {
                continue;
            }
            let result = self.blocks_to_fetch.contains_key(peer_index);
            if !result {
                // if we don't have an entry, we find the minimum block id for the peer
                let mut min_id = u64::MAX;
                for (id, hash) in map.iter() {
                    if min_id > *id {
                        min_id = *id;
                    }
                }
                let hash = map.remove(&min_id).unwrap();
                let vec = vec![(hash, false, min_id)];
                self.blocks_to_fetch.insert(*peer_index, vec);
            }
            loop {
                let result = self.blocks_to_fetch.get_mut(peer_index);
                if result.is_none() {
                    break;
                }
                let vec = result.unwrap();
                let (_, _, block_id) = vec.last().unwrap();
                let result = map.remove(&(*block_id + 1));
                if result.is_none() {
                    break;
                }
                let result = result.unwrap();
                vec.push((result, false, *block_id + 1));
            }
        }
        self.received_block_picture
            .retain(|index, map| !map.is_empty());
        self.blocks_to_fetch.retain(|index, vec| !vec.is_empty());
    }

    pub fn request_blocks_from_waitlist(&mut self) -> HashMap<u64, Vec<SaitoHash>> {
        debug!("requesting blocks from waiting list");
        let mut res: HashMap<u64, Vec<SaitoHash>> = Default::default();

        // for each peer check if we can fetch block
        for (peer_index, hashes) in self.blocks_to_fetch.iter_mut() {
            // check if we have blocks to fetch within our batch size
            for i in 0..min(hashes.len(), self.batch_size) {
                // TODO : same block can be fetched from multiple peers as of now. need to define the expected behaviour
                let (hash, fetching, block_id) = hashes
                    .get_mut(i)
                    .expect("entry should exist since we are checking the length");
                if *block_id > self.block_ceiling {
                    break;
                }
                if !*fetching {
                    debug!("fetching : {:?}", hex::encode(*hash));
                    if res.contains_key(peer_index) {
                        let vec = res.get_mut(&peer_index).unwrap();
                        vec.push(*hash);
                    } else {
                        let vec = vec![hash.clone()];
                        res.insert(*peer_index, vec);
                    }
                }
            }
        }

        res
    }
    pub fn mark_as_fetching(&mut self, entries: Vec<(u64, SaitoHash)>) {
        for (peer_index, hash) in entries.iter() {
            let res = self.blocks_to_fetch.get_mut(peer_index).unwrap();
            for (block_hash, fetching, block_id) in res {
                if hash.eq(block_hash) {
                    *fetching = true;
                    break;
                }
            }
        }
    }
    pub fn add_entry(&mut self, block_hash: SaitoHash, block_id: u64, peer_index: u64) {
        let result = self.received_block_picture.get_mut(&peer_index);
        if result.is_none() {
            let mut map: HashMap<u64, SaitoHash> = Default::default();
            map.insert(block_id, block_hash);
            self.received_block_picture.insert(peer_index, map);
        } else {
            let map = result.unwrap();
            map.insert(block_id, block_hash);
        }
    }
    pub fn remove_entry(&mut self, block_hash: SaitoHash, peer_index: u64) {
        let hashes = self.blocks_to_fetch.get_mut(&peer_index);
        if hashes.is_some() {
            let hashes = hashes.unwrap();
            hashes.retain(|(hash, _, _)| !block_hash.eq(hash));
        }
        self.blocks_to_fetch.retain(|index, map| !map.is_empty());
    }
    pub fn get_stats(&self) -> Vec<String> {
        let mut stats = vec![];
        for (peer_index, vec) in self.blocks_to_fetch.iter() {
            let res = self.received_block_picture.get(peer_index);
            let mut count = 0;
            if res.is_some() {
                count = res.unwrap().len();
            }
            let mut last_id = 0;
            let last = vec.last();
            if last.is_some() {
                last_id = last.unwrap().2;
            }
            let mut first_id = 0;
            let first = vec.first();
            if first.is_some() {
                first_id = first.unwrap().2;
            }
            let stat = format!(
                "--- stats ------ {} - peer : {:?} first: {:?} fetching_count : {:?} ordered_till : {:?} waiting_to_order : {:?}",
                format!("{:width$}", "routing:sync_state", width = 30),
                peer_index,
                first_id,
                vec.len(),
                last_id,
                count
            );
            stats.push(stat);
        }
        let stat = format!(
            "--- stats ------ {} - block_ceiling : {:?}",
            format!("{:width$}", "routing:sync_state", width = 30),
            self.block_ceiling
        );
        stats.push(stat);
        stats
    }
    pub fn set_latest_blockchain_id(&mut self, id: u64) {
        self.block_ceiling = id + self.batch_size as u64;
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::blockchain_sync_state::BlockchainSyncState;

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
}

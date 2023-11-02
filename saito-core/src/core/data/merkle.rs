// use std::collections::LinkedList;

// use log::debug;
// use rayon::prelude::*;

// use crate::common::defs::SaitoHash;
// use crate::core::data::crypto::hash;
// use crate::core::data::transaction::Transaction;
// use crate::iterate_mut;

// #[derive(PartialEq)]
// pub enum TraverseMode {
//     DepthFist,
//     BreadthFirst,
// }

// enum NodeType {
//     Node {
//         left: Option<Box<MerkleTreeNode>>,
//         right: Option<Box<MerkleTreeNode>>,
//     },
//     Transaction {
//         index: usize,
//     },
// }

// pub struct MerkleTreeNode {
//     node_type: NodeType,
//     hash: Option<SaitoHash>,
//     count: usize,
//     is_spv: bool, // New field indicating if this node is an SPV node
// }

// impl MerkleTreeNode {
//     fn new(
//         node_type: NodeType,
//         hash: Option<SaitoHash>,
//         count: usize,
//         is_spv: bool,
//     ) -> MerkleTreeNode {
//         MerkleTreeNode {
//             node_type,
//             hash,
//             count,
//             is_spv, // Initialize the new field with the provided value
//         }
//     }

//     pub fn get_hash(&self) -> Option<SaitoHash> {
//         return self.hash;
//     }

//     // New method to check if this node is an SPV node
//     pub fn is_spv_node(&self) -> bool {
//         return self.is_spv;
//     }
// }

// pub struct MerkleTree {
//     root: Box<MerkleTreeNode>,
// }

// impl MerkleTree {
//     pub fn len(&self) -> usize {
//         self.root.count
//     }

//     pub fn get_root_hash(&self) -> SaitoHash {
//         return self.root.hash.unwrap();
//     }
//     pub fn generate(transactions: &Vec<Transaction>) -> Option<Box<MerkleTree>> {
//         if transactions.is_empty() {
//             return None;
//         }

//         debug!("Generating merkle tree");

//         let mut leaves: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

//         // Handle the creation of leaves, including SPV leaves.
//         for index in 0..transactions.len() {
//             if transactions[index].txs_replacements > 1 {
//                 // For SPV transactions, add the appropriate number of ghost leaves.
//                 for _ in 0..transactions[index].txs_replacements {
//                     leaves.push_back(Box::new(MerkleTreeNode::new(
//                         NodeType::Transaction { index }, // Representing an SPV transaction
//                         Some(transactions[index].hash_for_signature.unwrap_or([0; 32])), // Use the SPV transaction's hash
//                         1,
//                         true, // is_spv
//                     )));
//                 }
//             } else {
//                 // For non-SPV transactions, add a single leaf with its hash.
//                 leaves.push_back(Box::new(MerkleTreeNode::new(
//                     NodeType::Transaction { index },
//                     transactions[index].hash_for_signature,
//                     1,
//                     false, // is_spv
//                 )));
//             }
//         }

//         // Build the tree layer by layer.
//         while leaves.len() > 1 {
//             let mut nodes: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

//             // Combine leaves into nodes.
//             while leaves.len() > 1 {
//                 let left_leaf = leaves.pop_front().unwrap();
//                 let right_leaf = leaves.pop_front().unwrap();

//                 // Determine if the new node is an SPV node.
//                 let is_spv = left_leaf.is_spv && right_leaf.is_spv;
//                 let combined_hash = if is_spv {
//                     // If both are SPV, we do not need to hash them again.
//                     left_leaf.get_hash()
//                 } else {
//                     // If not, compute the hash for the new node.
//                     // You would call your hash function here with left and right leaf hashes.
//                     Some(Self::compute_combined_hash(
//                         left_leaf.get_hash(),
//                         right_leaf.get_hash(),
//                     ))
//                 };

//                 // Create the new node with combined children and hash.
//                 nodes.push_back(Box::new(MerkleTreeNode::new(
//                     NodeType::Node {
//                         left: Some(left_leaf),
//                         right: Some(right_leaf),
//                     },
//                     combined_hash,
//                     2, // Count is always 2 for a combined node
//                     is_spv,
//                 )));
//             }

//             // If there's an odd number of leaves, the last one gets moved up without a pair.
//             if let Some(leaf) = leaves.pop_front() {
//                 nodes.push_back(leaf);
//             }

//             // The newly created nodes become the leaves for the next iteration.
//             leaves = nodes;
//         }

//         Some(Box::new(MerkleTree {
//             root: leaves.pop_front().unwrap(),
//         }))
//     }

//     fn compute_combined_hash(
//         left_hash: Option<[u8; 32]>,
//         right_hash: Option<[u8; 32]>,
//     ) -> [u8; 32] {
//         let mut vbytes: Vec<u8> = vec![];
//         vbytes.extend(left_hash.unwrap());
//         vbytes.extend(right_hash.unwrap());
//         hash(&vbytes)
//     }

//     pub fn traverse(&self, mode: TraverseMode, read_func: impl Fn(&MerkleTreeNode)) {
//         MerkleTree::traverse_node(&mode, &self.root, &read_func);
//     }

//     pub fn create_clone(&self) -> Box<MerkleTree> {
//         return Box::new(MerkleTree {
//             root: MerkleTree::clone_node(Some(&self.root)).unwrap(),
//         });
//     }

//     pub fn prune(&mut self, prune_func: impl Fn(usize) -> bool) {
//         MerkleTree::prune_node(Some(&mut self.root), &prune_func);
//     }

//     fn calculate_child_count(
//         left: &Option<Box<MerkleTreeNode>>,
//         right: &Option<Box<MerkleTreeNode>>,
//     ) -> usize {
//         let mut count = 1 as usize;

//         if left.is_some() {
//             count += left.as_ref().unwrap().count;
//         }

//         if right.is_some() {
//             count += right.as_ref().unwrap().count;
//         }

//         return count;
//     }

//     fn generate_hash(node: &mut MerkleTreeNode) -> bool {
//         if node.hash.is_some() {
//             return true;
//         }

//         match &node.node_type {
//             NodeType::Node { left, right } => {
//                 let mut vbytes: Vec<u8> = vec![];
//                 vbytes.extend(left.as_ref().unwrap().hash.unwrap());
//                 vbytes.extend(right.as_ref().unwrap().hash.unwrap());
//                 node.hash = Some(hash(&vbytes));
//                 // trace!(
//                 //     "Node : buffer = {:?}, hash = {:?}",
//                 //     hex::encode(vbytes),
//                 //     hex::encode(node.hash.unwrap())
//                 // );
//             }
//             NodeType::Transaction { .. } => {}
//         }

//         return true;
//     }

//     fn traverse_node(
//         mode: &TraverseMode,
//         node: &MerkleTreeNode,
//         read_func: &impl Fn(&MerkleTreeNode),
//     ) {
//         match mode {
//             TraverseMode::DepthFist => {
//                 // For pre-order traversal, process the node before its children.
//                 read_func(node);

//                 if let NodeType::Node { left, right } = &node.node_type {
//                     if let Some(left_node) = left {
//                         MerkleTree::traverse_node(mode, left_node, read_func);
//                     }
//                     if let Some(right_node) = right {
//                         MerkleTree::traverse_node(mode, right_node, read_func);
//                     }
//                 }

//                 // For in-order or post-order traversal, process the node here.
//             }
//             TraverseMode::BreadthFirst => {
//                 // Using a queue for the breadth-first traversal.
//                 let mut queue = std::collections::VecDeque::new();
//                 queue.push_back(node);

//                 while let Some(current_node) = queue.pop_front() {
//                     read_func(current_node);

//                     if let NodeType::Node { left, right } = &current_node.node_type {
//                         if let Some(left_node) = left {
//                             queue.push_back(left_node);
//                         }
//                         if let Some(right_node) = right {
//                             queue.push_back(right_node);
//                         }
//                     }
//                 }
//             }
//         }
//     }
//     fn clone_node(node: Option<&Box<MerkleTreeNode>>) -> Option<Box<MerkleTreeNode>> {
//         if node.is_some() {
//             Some(Box::new(MerkleTreeNode::new(
//                 match &node.unwrap().node_type {
//                     NodeType::Node { left, right } => NodeType::Node {
//                         left: MerkleTree::clone_node(left.as_ref()),
//                         right: MerkleTree::clone_node(right.as_ref()),
//                     },
//                     NodeType::Transaction { index } => NodeType::Transaction { index: *index },
//                 },
//                 node.as_ref().unwrap().hash,
//                 node.as_ref().unwrap().count,
//                 node.as_ref().unwrap().is_spv,
//             )))
//         } else {
//             None
//         }
//     }

//     fn prune_node(
//         node: Option<&mut Box<MerkleTreeNode>>,
//         prune_func: &impl Fn(usize) -> bool,
//     ) -> bool {
//         return if node.is_some() {
//             let node = node.unwrap();
//             match &mut node.node_type {
//                 NodeType::Node { left, right } => {
//                     let mut prune = MerkleTree::prune_node(left.as_mut(), prune_func);
//                     prune &= MerkleTree::prune_node(right.as_mut(), prune_func);

//                     if prune {
//                         node.node_type = NodeType::Node {
//                             left: None,
//                             right: None,
//                         };
//                         node.count = 1;
//                     } else {
//                         node.count = MerkleTree::calculate_child_count(&left, &right);
//                     }

//                     prune
//                 }
//                 NodeType::Transaction { index } => prune_func(*index),
//             }
//         } else {
//             true
//         };
//     }
//     // Get Merkle path for a specific transaction in the tree
//     pub fn get_merkle_path(&self, target_tx_hash: &SaitoHash) -> Option<Vec<SaitoHash>> {
//         let mut path = Vec::new();
//         if self.retrieve_merkle_path(&self.root, target_tx_hash, &mut path) {
//             Some(path)
//         } else {
//             None
//         }
//     }

//     /// Recursive helper function to retrieve a Merkle path for a specific transaction
//     fn retrieve_merkle_path(
//         &self,
//         node: &MerkleTreeNode,
//         target_tx_hash: &SaitoHash,
//         path: &mut Vec<SaitoHash>,
//     ) -> bool {
//         match &node.node_type {
//             NodeType::Transaction { .. } => node.hash.as_ref() == Some(target_tx_hash),
//             NodeType::Node { left, right } => {
//                 if let Some(left_node) = left {
//                     if self.retrieve_merkle_path(left_node, target_tx_hash, path) {
//                         if let Some(right_node) = right {
//                             if let Some(hash) = &right_node.hash {
//                                 path.push(*hash);
//                             }
//                         }
//                         return true;
//                     }
//                 }
//                 if let Some(right_node) = right {
//                     if self.retrieve_merkle_path(right_node, target_tx_hash, path) {
//                         if let Some(left_node) = left {
//                             if let Some(hash) = &left_node.hash {
//                                 path.push(*hash);
//                             }
//                         }
//                         return true;
//                     }
//                 }
//                 false
//             }
//         }
//     }

//     /// Verify a transaction using a Merkle path
//     pub fn verify_merkle_path(
//         merkle_root: &SaitoHash,
//         tx_hash: &SaitoHash,
//         merkle_path: &[SaitoHash],
//     ) -> bool {
//         let mut current_hash = tx_hash.clone();
//         for path_hash in merkle_path {
//             let mut combined = current_hash.to_vec();
//             combined.extend_from_slice(path_hash);
//             current_hash = hash(&combined);
//         }
//         &current_hash == merkle_root
//     }
//     pub fn get_merkle_path_for_transaction(&self, tx: &Transaction) -> Option<Vec<SaitoHash>> {
//         let mut path = Vec::new();
//         if self.find_path(&self.root, &tx.hash_for_signature.unwrap(), &mut path) {
//             Some(path)
//         } else {
//             dbg!("None");
//             None
//         }
//     }

//     fn find_path(
//         &self,
//         node: &Box<MerkleTreeNode>,
//         target_hash: &SaitoHash,
//         path: &mut Vec<SaitoHash>,
//     ) -> bool {
//         match &node.node_type {
//             NodeType::Transaction { .. } => node.hash.as_ref() == Some(target_hash),
//             NodeType::Node { left, right } => {
//                 // Check the left child for the presence of the target hash
//                 if let Some(left_node) = left {
//                     if self.is_hash_present(&left_node, target_hash) {
//                         // Push the right sibling's hash if it's not an SPV node or the target
//                         if let Some(right_node) = right {
//                             if !right_node.is_spv && right_node.hash.as_ref() != Some(target_hash) {
//                                 path.push(right_node.hash.expect("Right node must have a hash"));
//                             }
//                         }
//                         return self.find_path(left_node, target_hash, path);
//                     }
//                 }

//                 // Check the right child for the presence of the target hash
//                 if let Some(right_node) = right {
//                     if self.is_hash_present(&right_node, target_hash) {
//                         // Push the left sibling's hash if it's not an SPV node or the target
//                         if let Some(left_node) = left {
//                             if !left_node.is_spv && left_node.hash.as_ref() != Some(target_hash) {
//                                 path.push(left_node.hash.expect("Left node must have a hash"));
//                             }
//                         }
//                         return self.find_path(right_node, target_hash, path);
//                     }
//                 }

//                 false
//             }
//         }
//     }

//     fn is_hash_present(&self, node: &Box<MerkleTreeNode>, target_hash: &SaitoHash) -> bool {
//         match &node.node_type {
//             NodeType::Transaction { .. } => node.hash.as_ref() == Some(target_hash),
//             NodeType::Node { left, right } => {
//                 // Check if the target_hash is present in the left subtree
//                 left.as_ref().map_or(false, |ln| self.is_hash_present(ln, target_hash)) ||
//                 // Check if the target_hash is present in the right subtree
//                 right.as_ref().map_or(false, |rn| self.is_hash_present(rn, target_hash))
//             }
//         }
//     }

//     pub fn construct_merkle_proof(&self, target_hash: &SaitoHash) -> Option<Vec<SaitoHash>> {
//         let mut path = Vec::new();
//         if !self.is_hash_present(&self.root, target_hash) {
//             return None; // The target hash is not in this tree.
//         }

//         // Recursive function to walk the tree and build the path.
//         fn build_path(
//             node: &Box<MerkleTreeNode>,
//             target_hash: &SaitoHash,
//             path: &mut Vec<SaitoHash>,
//         ) -> bool {
//             match &node.node_type {
//                 NodeType::Transaction { .. } => {
//                     // Leaf node, check if this is the transaction we're building the proof for.
//                     node.hash.unwrap() == *target_hash
//                 }
//                 NodeType::Node { left, right } => {
//                     // Internal node, recursively search for the transaction.
//                     if let Some(left) = left {
//                         if build_path(left, target_hash, path) {
//                             // The transaction is in the left subtree, add the right hash to the path.
//                             if let Some(right) = right {
//                                 path.push(right.hash.unwrap());
//                             }
//                             return true;
//                         }
//                     }
//                     if let Some(right) = right {
//                         if build_path(right, target_hash, path) {
//                             // The transaction is in the right subtree, add the left hash to the path.
//                             if let Some(left) = left {
//                                 path.push(left.hash.unwrap());
//                             }
//                             return true;
//                         }
//                     }
//                     false
//                 }
//             }
//         }

//         // Initiate the recursive search.
//         if build_path(&self.root, target_hash, &mut path) {
//             Some(path)
//         } else {
//             None // No path found, which should not happen if is_hash_present returned true.
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use crate::common::defs::SaitoHash;
//     use crate::core::data::crypto::{generate_keys, hash};
//     use crate::core::data::merkle::MerkleTree;
//     use crate::core::data::transaction::Transaction;
//     use crate::core::data::wallet::Wallet;

//     #[test]
//     fn merkle_tree_generation_test() {
//         let keys = generate_keys();
//         let wallet = Wallet::new(keys.1, keys.0);

//         let mut transactions = vec![];

//         for i in 0..5 {
//             let mut transaction = Transaction::default();
//             transaction.timestamp = i;
//             transaction.sign(&wallet.private_key);
//             transactions.push(transaction);
//         }

//         let tree1 = MerkleTree::generate(&transactions).unwrap();

//         transactions[0].timestamp = 10;
//         transactions[0].sign(&wallet.private_key);
//         let tree2 = MerkleTree::generate(&transactions).unwrap();

//         transactions[4].timestamp = 11;
//         transactions[4].sign(&wallet.private_key);
//         let tree3 = MerkleTree::generate(&transactions).unwrap();

//         transactions[2].timestamp = 12;
//         transactions[2].sign(&wallet.private_key);
//         let tree4 = MerkleTree::generate(&transactions).unwrap();
//         let tree5 = MerkleTree::generate(&transactions).unwrap();

//         assert_ne!(tree1.get_root_hash(), tree2.get_root_hash());
//         assert_ne!(tree2.get_root_hash(), tree3.get_root_hash());
//         assert_ne!(tree3.get_root_hash(), tree4.get_root_hash());

//         assert_eq!(tree4.get_root_hash(), tree5.get_root_hash());

//         assert_eq!(tree1.len(), 11);
//         assert_eq!(tree2.len(), 11);
//         assert_eq!(tree3.len(), 11);
//         assert_eq!(tree4.len(), 11);
//         assert_eq!(tree5.len(), 11);
//     }

//     #[test]
//     fn merkle_tree_pruning_test() {
//         let keys = generate_keys();
//         let wallet = Wallet::new(keys.1, keys.0);

//         let mut transactions = vec![];

//         for i in 0..5 {
//             let mut transaction = Transaction::default();
//             transaction.timestamp = i;
//             transaction.sign(&wallet.private_key);
//             transactions.push(transaction);
//         }

//         let target_hash = transactions[0].hash_for_signature.unwrap();

//         let tree = MerkleTree::generate(&transactions).unwrap();
//         let cloned_tree = tree.create_clone();
//         let mut pruned_tree = tree.create_clone();
//         pruned_tree.prune(|index| target_hash != transactions[index].hash_for_signature.unwrap());

//         assert_eq!(tree.get_root_hash(), cloned_tree.get_root_hash());
//         assert_eq!(cloned_tree.get_root_hash(), pruned_tree.get_root_hash());
//         assert_eq!(tree.len(), 11);
//         assert_eq!(cloned_tree.len(), tree.len());
//         assert_eq!(pruned_tree.len(), 7);

//         // println!("\ntree");
//         // tree.traverse(TraverseMode::BreadthFirst, |node| {
//         //     print!("{}, ", hex::encode(node.hash.unwrap()))
//         // });
//         //
//         // println!("\ncloned_tree");
//         // cloned_tree.traverse(TraverseMode::BreadthFirst, |node| {
//         //     print!("{}, ", hex::encode(node.hash.unwrap()))
//         // });
//         //
//         // println!("\npruned_tree");
//         // pruned_tree.traverse(TraverseMode::BreadthFirst, |node| {
//         //     print!("{}, ", hex::encode(node.hash.unwrap()))
//         // });
//     }
// }

use std::collections::LinkedList;

use log::debug;
use rayon::prelude::*;

use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
use crate::iterate_mut;

#[derive(PartialEq)]
pub enum TraverseMode {
    DepthFist,
    BreadthFirst,
}

enum NodeType {
    Node {
        left: Option<Box<MerkleTreeNode>>,
        right: Option<Box<MerkleTreeNode>>,
    },
    Transaction {
        index: usize,
    },
}

pub struct MerkleTreeNode {
    node_type: NodeType,
    hash: Option<SaitoHash>,
    count: usize,
    is_spv: bool,
}

impl MerkleTreeNode {
    fn new(
        node_type: NodeType,
        hash: Option<SaitoHash>,
        count: usize,
        is_spv: bool,
    ) -> MerkleTreeNode {
        MerkleTreeNode {
            node_type,
            hash,
            count,
            is_spv,
        }
    }

    pub fn get_hash(&self) -> Option<SaitoHash> {
        return self.hash;
    }
}

pub struct MerkleTree {
    root: Box<MerkleTreeNode>,
}

impl MerkleTree {
    pub fn len(&self) -> usize {
        self.root.count
    }

    pub fn get_root_hash(&self) -> SaitoHash {
        return self.root.hash.unwrap();
    }

    pub fn generate(transactions: &Vec<Transaction>) -> Option<Box<MerkleTree>> {
        if transactions.is_empty() {
            return None;
        }

        debug!("Generating merkle tree");

        let mut leaves: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

        // Handle the creation of leaves, including SPV leaves.
        for index in 0..transactions.len() {
            if transactions[index].txs_replacements > 1 {
                // For SPV transactions, add the appropriate number of ghost leaves.
                for _ in 0..transactions[index].txs_replacements {
                    leaves.push_back(Box::new(MerkleTreeNode::new(
                        NodeType::Transaction { index }, // Representing an SPV transaction
                        Some(transactions[index].hash_for_signature.unwrap_or([0; 32])), // Use the SPV transaction's hash
                        1,
                        true, // is_spv
                    )));
                }
            } else {
                // For non-SPV transactions, add a single leaf with its hash.
                leaves.push_back(Box::new(MerkleTreeNode::new(
                    NodeType::Transaction { index },
                    transactions[index].hash_for_signature,
                    1,
                    false, // is_spv
                )));
            }
        }

        // Build the tree layer by layer.
        while leaves.len() > 1 {
            let mut nodes: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

            // Combine leaves into nodes.
            while leaves.len() > 1 {
                let left_leaf = leaves.pop_front().unwrap();
                let right_leaf = leaves.pop_front().unwrap();

                // Determine if the new node is an SPV node.
                let is_spv = left_leaf.is_spv && right_leaf.is_spv;
                let combined_hash = if is_spv {
                    // If both are SPV, we do not need to hash them again.
                    left_leaf.get_hash()
                } else {
                    // If not, compute the hash for the new node.
                    // You would call your hash function here with left and right leaf hashes.
                    Some(Self::compute_combined_hash(
                        left_leaf.get_hash(),
                        right_leaf.get_hash(),
                    ))
                };

                // Create the new node with combined children and hash.
                nodes.push_back(Box::new(MerkleTreeNode::new(
                    NodeType::Node {
                        left: Some(left_leaf),
                        right: Some(right_leaf),
                    },
                    combined_hash,
                    2, // Count is always 2 for a combined node
                    is_spv,
                )));
            }

            // If there's an odd number of leaves, the last one gets moved up without a pair.
            if let Some(leaf) = leaves.pop_front() {
                nodes.push_back(leaf);
            }

            // The newly created nodes become the leaves for the next iteration.
            leaves = nodes;
        }

        Some(Box::new(MerkleTree {
            root: leaves.pop_front().unwrap(),
        }))
    }

    fn compute_combined_hash(
        left_hash: Option<[u8; 32]>,
        right_hash: Option<[u8; 32]>,
    ) -> [u8; 32] {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(left_hash.unwrap());
        vbytes.extend(right_hash.unwrap());
        hash(&vbytes)
    }
    pub fn traverse(&self, mode: TraverseMode, read_func: impl Fn(&MerkleTreeNode)) {
        MerkleTree::traverse_node(&mode, &self.root, &read_func);
    }

    pub fn create_clone(&self) -> Box<MerkleTree> {
        return Box::new(MerkleTree {
            root: MerkleTree::clone_node(Some(&self.root)).unwrap(),
        });
    }

    pub fn prune(&mut self, prune_func: impl Fn(usize) -> bool) {
        MerkleTree::prune_node(Some(&mut self.root), &prune_func);
    }

    fn calculate_child_count(
        left: &Option<Box<MerkleTreeNode>>,
        right: &Option<Box<MerkleTreeNode>>,
    ) -> usize {
        let mut count = 1 as usize;

        if left.is_some() {
            count += left.as_ref().unwrap().count;
        }

        if right.is_some() {
            count += right.as_ref().unwrap().count;
        }

        return count;
    }

    fn generate_hash(node: &mut MerkleTreeNode) -> bool {
        if node.hash.is_some() {
            return true;
        }

        match &node.node_type {
            NodeType::Node { left, right } => {
                let mut vbytes: Vec<u8> = vec![];
                vbytes.extend(left.as_ref().unwrap().hash.unwrap());
                vbytes.extend(right.as_ref().unwrap().hash.unwrap());
                node.hash = Some(hash(&vbytes));
                // trace!(
                //     "Node : buffer = {:?}, hash = {:?}",
                //     hex::encode(vbytes),
                //     hex::encode(node.hash.unwrap())
                // );
            }
            NodeType::Transaction { .. } => {}
        }

        return true;
    }

    fn traverse_node(
        mode: &TraverseMode,
        node: &MerkleTreeNode,
        read_func: &impl Fn(&MerkleTreeNode),
    ) {
        if *mode == TraverseMode::BreadthFirst {
            read_func(node);
        }

        match &node.node_type {
            NodeType::Node { left, right } => {
                if left.is_some() {
                    MerkleTree::traverse_node(mode, &left.as_ref().unwrap(), read_func);
                }

                if right.is_some() {
                    MerkleTree::traverse_node(mode, &right.as_ref().unwrap(), read_func);
                }
            }
            NodeType::Transaction { .. } => {}
        }

        if *mode == TraverseMode::DepthFist {
            read_func(node);
        }
    }

    fn clone_node(node: Option<&Box<MerkleTreeNode>>) -> Option<Box<MerkleTreeNode>> {
        if node.is_some() {
            Some(Box::new(MerkleTreeNode::new(
                match &node.unwrap().node_type {
                    NodeType::Node { left, right } => NodeType::Node {
                        left: MerkleTree::clone_node(left.as_ref()),
                        right: MerkleTree::clone_node(right.as_ref()),
                    },
                    NodeType::Transaction { index } => NodeType::Transaction { index: *index },
                },
                node.as_ref().unwrap().hash,
                node.as_ref().unwrap().count,
                node.as_ref().unwrap().is_spv,
            )))
        } else {
            None
        }
    }

    fn prune_node(
        node: Option<&mut Box<MerkleTreeNode>>,
        prune_func: &impl Fn(usize) -> bool,
    ) -> bool {
        return if node.is_some() {
            let node = node.unwrap();
            match &mut node.node_type {
                NodeType::Node { left, right } => {
                    let mut prune = MerkleTree::prune_node(left.as_mut(), prune_func);
                    prune &= MerkleTree::prune_node(right.as_mut(), prune_func);

                    if prune {
                        node.node_type = NodeType::Node {
                            left: None,
                            right: None,
                        };
                        node.count = 1;
                    } else {
                        node.count = MerkleTree::calculate_child_count(&left, &right);
                    }

                    prune
                }
                NodeType::Transaction { index } => prune_func(*index),
            }
        } else {
            true
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::crypto::generate_keys;
    use crate::core::data::merkle::MerkleTree;
    use crate::core::data::transaction::Transaction;
    use crate::core::data::wallet::Wallet;

    #[test]
    fn merkle_tree_generation_test() {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        let mut transactions = vec![];

        for i in 0..5 {
            let mut transaction = Transaction::default();
            transaction.timestamp = i;
            transaction.sign(&wallet.private_key);
            transactions.push(transaction);
        }

        let tree1 = MerkleTree::generate(&transactions).unwrap();

        transactions[0].timestamp = 10;
        transactions[0].sign(&wallet.private_key);
        let tree2 = MerkleTree::generate(&transactions).unwrap();

        transactions[4].timestamp = 11;
        transactions[4].sign(&wallet.private_key);
        let tree3 = MerkleTree::generate(&transactions).unwrap();

        transactions[2].timestamp = 12;
        transactions[2].sign(&wallet.private_key);
        let tree4 = MerkleTree::generate(&transactions).unwrap();
        let tree5 = MerkleTree::generate(&transactions).unwrap();

        assert_ne!(tree1.get_root_hash(), tree2.get_root_hash());
        assert_ne!(tree2.get_root_hash(), tree3.get_root_hash());
        assert_ne!(tree3.get_root_hash(), tree4.get_root_hash());

        assert_eq!(tree4.get_root_hash(), tree5.get_root_hash());

        assert_eq!(tree1.len(), 11);
        assert_eq!(tree2.len(), 11);
        assert_eq!(tree3.len(), 11);
        assert_eq!(tree4.len(), 11);
        assert_eq!(tree5.len(), 11);
    }

    #[test]
    fn merkle_tree_pruning_test() {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        let mut transactions = vec![];

        for i in 0..5 {
            let mut transaction = Transaction::default();
            transaction.timestamp = i;
            transaction.sign(&wallet.private_key);
            transactions.push(transaction);
        }

        let target_hash = transactions[0].hash_for_signature.unwrap();

        let tree = MerkleTree::generate(&transactions).unwrap();
        let cloned_tree = tree.create_clone();
        let mut pruned_tree = tree.create_clone();
        pruned_tree.prune(|index| target_hash != transactions[index].hash_for_signature.unwrap());

        assert_eq!(tree.get_root_hash(), cloned_tree.get_root_hash());
        assert_eq!(cloned_tree.get_root_hash(), pruned_tree.get_root_hash());
        assert_eq!(tree.len(), 11);
        assert_eq!(cloned_tree.len(), tree.len());
        assert_eq!(pruned_tree.len(), 7);

        // println!("\ntree");
        // tree.traverse(TraverseMode::BreadthFirst, |node| {
        //     print!("{}, ", hex::encode(node.hash.unwrap()))
        // });
        //
        // println!("\ncloned_tree");
        // cloned_tree.traverse(TraverseMode::BreadthFirst, |node| {
        //     print!("{}, ", hex::encode(node.hash.unwrap()))
        // });
        //
        // println!("\npruned_tree");
        // pruned_tree.traverse(TraverseMode::BreadthFirst, |node| {
        //     print!("{}, ", hex::encode(node.hash.unwrap()))
        // });
    }
}

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

    // pub fn generate(transactions: &Vec<Transaction>) -> Option<Box<MerkleTree>> {
    //     if transactions.is_empty() {
    //         return None;
    //     }

    //     debug!("Generating merkle tree");

    //     let mut leaves: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

    //     for index in 0..transactions.len() {
    //         if transactions[index].txs_replacements > 1 {
    //             for _ in 0..transactions[index].txs_replacements {
    //                 leaves.push_back(Box::new(MerkleTreeNode::new(
    //                     NodeType::Transaction { index },
    //                     Some(transactions[index].hash_for_signature.unwrap_or([0; 32])),
    //                     1,
    //                     true, // is_spv
    //                 )));
    //             }
    //         } else {
    //             leaves.push_back(Box::new(MerkleTreeNode::new(
    //                 NodeType::Transaction { index },
    //                 transactions[index].hash_for_signature,
    //                 1,
    //                 false, // is_spv
    //             )));
    //         }
    //     }

    //     while leaves.len() > 1 {
    //         let mut nodes: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

    //         // Combine leaves into nodes.
    //         while leaves.len() > 1 {
    //             let left_leaf = leaves.pop_front().unwrap();
    //             let right_leaf = leaves.pop_front().unwrap();

    //             // Determine if the new node is an SPV node and if we should skip hashing.
    //             let is_spv = left_leaf.is_spv && right_leaf.is_spv;
    //             if is_spv {
    //                 // If both are SPV transactions, push the left leaf only as the parent node.
    //                 nodes.push_back(left_leaf);
    //                 // Decide what to do with right_leaf - here we're discarding it
    //             } else {
    //                 let combined_hash = Some(Self::compute_combined_hash(
    //                     left_leaf.get_hash(),
    //                     right_leaf.get_hash(),
    //                 ));
    //                 // Create the new node with combined children and hash.
    //                 nodes.push_back(Box::new(MerkleTreeNode::new(
    //                     NodeType::Node {
    //                         left: Some(left_leaf),
    //                         right: Some(right_leaf),
    //                     },
    //                     combined_hash,
    //                     2,
    //                     false,
    //                 )));
    //             }
    //         }

    //         if let Some(leaf) = leaves.pop_front() {
    //             nodes.push_back(leaf);
    //         }

    //         leaves = nodes;
    //     }

    //     Some(Box::new(MerkleTree {
    //         root: leaves.pop_front().unwrap(),
    //     }))
    // }

    pub fn generate(transactions: &Vec<Transaction>) -> Option<Box<MerkleTree>> {
        if transactions.is_empty() {
            return None;
        }

        debug!("Generating merkle tree");

        let mut leaves: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

        // Create leaves for the Merkle tree
        for index in 0..transactions.len() {
            let tx_replacements = std::cmp::max(1, transactions[index].txs_replacements);
            for _ in 0..tx_replacements {
                leaves.push_back(Box::new(MerkleTreeNode::new(
                    NodeType::Transaction { index },
                    transactions[index].hash_for_signature,
                    1,
                    tx_replacements > 1, // is_spv if txs_replacements > 1
                )));
            }
        }

        // Combine leaves into nodes to form the tree
        while leaves.len() > 1 {
            let mut nodes: LinkedList<Box<MerkleTreeNode>> = LinkedList::new();

            while leaves.len() > 0 {
                let left_leaf = leaves.pop_front().unwrap();
                let right_leaf = if leaves.len() > 0 {
                    leaves.pop_front().unwrap()
                } else {
                    // If there is an odd number of leaves, duplicate the last leaf
                    left_leaf.clone()
                };

                let combined_hash = Some(Self::compute_combined_hash(
                    left_leaf.get_hash(),
                    right_leaf.get_hash(),
                ));

                nodes.push_back(Box::new(MerkleTreeNode::new(
                    NodeType::Node {
                        left: Some(left_leaf),
                        right: Some(right_leaf),
                    },
                    combined_hash,
                    left_leaf.txs_replacements + right_leaf.txs_replacements,
                    false, // Nodes created from combining are not SPV
                )));
            }

            leaves = nodes;
        }

        Some(Box::new(MerkleTree {
            root: leaves.pop_front().unwrap(),
        }))
    }

    pub fn compute_combined_hash(
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

    pub fn calculate_child_count(
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
    // Generates a Merkle proof for the given transaction hash.
}

#[cfg(test)]
mod tests {
    use crate::core::data::crypto::generate_keys;
    use crate::core::data::merkle::MerkleTree;
    use crate::core::data::transaction::{Transaction, TransactionType};
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

        dbg!(tree1.get_root_hash(), tree2.get_root_hash());
        assert_ne!(tree1.get_root_hash(), tree2.get_root_hash());
        assert_ne!(tree2.get_root_hash(), tree3.get_root_hash());
        assert_ne!(tree3.get_root_hash(), tree4.get_root_hash());
        assert_eq!(tree4.get_root_hash(), tree5.get_root_hash());
    }

    #[test]
    fn test_generate_odd_number_of_transactions() {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        let mut transactions = Vec::new();
        for i in 0..3 {
            // Use 3 for an odd number of transactions
            let mut transaction = Transaction::default();
            transaction.timestamp = i as u64;
            transaction.sign(&wallet.private_key);
            transactions.push(transaction);
        }

        // Generate the Merkle tree from the transactions.
        let tree = MerkleTree::generate(&transactions).unwrap();

        let root_hash = tree.get_root_hash();

        assert_ne!(root_hash, [0u8; 32], "Root hash should not be all zeros.");

        let mut altered_transactions = transactions.clone();
        altered_transactions[0].timestamp += 1;
        altered_transactions[0].sign(&wallet.private_key);
        let altered_tree = MerkleTree::generate(&altered_transactions).unwrap();
        let altered_root_hash = altered_tree.get_root_hash();
        assert_ne!(
            root_hash, altered_root_hash,
            "Root hash should change when a transaction is altered."
        );
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
    }

    #[test]
    fn test_generate_with_spv_transactions() {
        let keys = generate_keys();
        let wallet = Wallet::new(keys.1, keys.0);

        let mut transactions = Vec::new();

        let mut normal_tx1 = Transaction::default();
        normal_tx1.sign(&wallet.private_key);
        transactions.push(normal_tx1.clone());

        let mut normal_tx2 = Transaction::default();
        normal_tx2.sign(&wallet.private_key);
        transactions.push(normal_tx2.clone());

        let mut normal_tx3 = Transaction::default();
        normal_tx3.sign(&wallet.private_key);
        transactions.push(normal_tx3.clone());

        let mut normal_tx4 = Transaction::default();
        normal_tx4.sign(&wallet.private_key);
        transactions.push(normal_tx4.clone());

        let mut normal_tx5 = Transaction::default();
        normal_tx5.sign(&wallet.private_key);
        transactions.push(normal_tx5.clone());

        let merkle_tree0 = MerkleTree::generate(&transactions).unwrap();

        transactions[1] = create_spv_transaction(wallet, transactions[1].clone());

        let merkle_tree1 = MerkleTree::generate(&transactions).unwrap();
        dbg!(merkle_tree0.get_root_hash(), merkle_tree1.get_root_hash());

        let mut tx = Transaction::default();

        // transactions.push(&mut spv_tx);

        let expected_leaves: u32 = transactions
            .iter()
            .map(|tx| std::cmp::max(1, tx.txs_replacements))
            .sum();

        // assert_eq!(expected_leaves, 6);
    }

    fn create_spv_transaction(wallet: Wallet, mut tx: Transaction) -> Transaction {
        tx.sign(&wallet.private_key);
        let spv_tx = Transaction {
            timestamp: tx.timestamp,
            from: tx.from.clone(),
            to: tx.to.clone(),
            data: tx.data.clone(),
            transaction_type: TransactionType::SPV,
            txs_replacements: 5,
            signature: tx.signature.clone(),
            path: tx.path.clone(),
            hash_for_signature: tx.hash_for_signature.clone(),
            total_in: tx.total_in,
            total_out: tx.total_out,
            total_fees: tx.total_fees,
            total_work_for_me: tx.total_work_for_me,
            cumulative_fees: tx.cumulative_fees,
        };
        spv_tx
    }
}

use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
use rayon::prelude::*;
use std::collections::LinkedList;

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
}

impl MerkleTreeNode {
    fn new(node_type: NodeType, hash: Option<SaitoHash>, count: usize) -> MerkleTreeNode {
        MerkleTreeNode {
            node_type,
            hash,
            count,
        }
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

        // Logic: a single node will be created per two leaves, each node will generate
        // a hash by combining the hashes of the two leaves, in the next loop
        // those node hashes will used as the leaves for the next set of nodes,
        // i.e. leaf count is reduced by half on each loop, eventually ending up in 1

        let mut leaves: LinkedList<Box<MerkleTreeNode>> = Default::default();

        for index in 0..transactions.len() {
            leaves.push_back(Box::new(MerkleTreeNode::new(
                NodeType::Transaction { index },
                transactions[index].hash_for_signature,
                1 as usize,
            )));
        }

        while leaves.len() > 1 {
            let mut nodes: LinkedList<MerkleTreeNode> = Default::default();

            // Create a node per two leaves
            while !leaves.is_empty() {
                let left = leaves.pop_front();
                let right = leaves.pop_front(); //Can be None, this is expected
                let count = MerkleTree::calculate_child_count(&left, &right);
                nodes.push_back(MerkleTreeNode::new(
                    NodeType::Node { left, right },
                    None,
                    count,
                ));
            }

            // Compute the node hashes in parallel
            nodes
                .par_iter_mut()
                .all(|node| MerkleTree::generate_hash(node));
            // Collect the next set of leaves for the computation
            leaves.clear();

            while !nodes.is_empty() {
                leaves.push_back(Box::new(nodes.pop_front().unwrap()));
            }
        }

        return Some(Box::new(MerkleTree {
            root: leaves.pop_front().unwrap(),
        }));
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
                if let Some(leftNode) = left {
                    if let Some(rightNode) = right {
                        let mut vbytes: Vec<u8> = vec![];
                        vbytes.extend(leftNode.hash.unwrap());
                        vbytes.extend(rightNode.hash.unwrap());
                        node.hash = Some(hash(&vbytes));
                    } else {
                        node.hash = leftNode.hash;
                    }
                } else {
                    if let Some(rightNode) = right {
                        node.hash = rightNode.hash;
                    } else {
                        unreachable!("Both leaves of a merkle tree node are null")
                    }
                }
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
            let mut node = node.unwrap();
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

    use crate::core::data::merkle::{MerkleTree, TraverseMode};
    use crate::core::data::transaction::Transaction;
    use crate::core::data::wallet::Wallet;

    #[test]
    fn merkle_tree_generation_test() {
        let wallet = Wallet::new();

        let mut transactions = vec![];

        for i in 0..5 {
            let mut transaction = Transaction::new();
            transaction.timestamp = i;
            transaction.sign(wallet.private_key);
            transactions.push(transaction);
        }

        let tree1 = MerkleTree::generate(&transactions).unwrap();

        transactions[0].timestamp = 10;
        transactions[0].sign(wallet.private_key);
        let tree2 = MerkleTree::generate(&transactions).unwrap();

        transactions[4].timestamp = 11;
        transactions[4].sign(wallet.private_key);
        let tree3 = MerkleTree::generate(&transactions).unwrap();

        transactions[2].timestamp = 12;
        transactions[2].sign(wallet.private_key);
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
        let wallet = Wallet::new();

        let mut transactions = vec![];

        for i in 0..5 {
            let mut transaction = Transaction::new();
            transaction.timestamp = i;
            transaction.sign(wallet.private_key);
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

        println!("\ntree");
        tree.traverse(TraverseMode::BreadthFirst, |node| {
            print!("{}, ", hex::encode(node.hash.unwrap()))
        });

        println!("\ncloned_tree");
        cloned_tree.traverse(TraverseMode::BreadthFirst, |node| {
            print!("{}, ", hex::encode(node.hash.unwrap()))
        });

        println!("\npruned_tree");
        pruned_tree.traverse(TraverseMode::BreadthFirst, |node| {
            print!("{}, ", hex::encode(node.hash.unwrap()))
        });
    }
}

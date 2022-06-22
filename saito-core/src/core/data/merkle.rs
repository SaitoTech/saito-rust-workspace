use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
use blake3::IncrementCounter::No;
use rayon::prelude::*;
use std::collections::LinkedList;

//
// MerkleTreeLayer is a short implementation that uses the default
// Saito hashing algorithm. It is also written to take advantage of
// rayon parallelization.
//
//#[derive(PartialEq, Debug, Clone)]

pub struct MerkleTreeNode {
    left: Option<Box<MerkleTreeNode>>,
    right: Option<Box<MerkleTreeNode>>,
    hash: Option<SaitoHash>,
    transaction: bool,
    count: usize,
}

impl MerkleTreeNode {
    fn new(
        left: Option<Box<MerkleTreeNode>>,
        right: Option<Box<MerkleTreeNode>>,
        hash: Option<SaitoHash>,
        transaction: bool,
        count: usize,
    ) -> MerkleTreeNode {
        MerkleTreeNode {
            left,
            right,
            hash,
            transaction,
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

    pub fn get_root(&self) -> &Box<MerkleTreeNode> {
        return &self.root;
    }

    pub fn generate(transactions: &Vec<Transaction>) -> Option<Box<MerkleTree>> {
        if transactions.is_empty() {
            return None;
        }

        // Logic: a single node will be created per two leaves, each node will generate
        // a hash by combining the hashes of the two leaves, in the next loop
        // those node hashes will used as the leaves for the next set of nodes,
        // i.e. leaf count is reduced by half on each loop, eventually ending up in 1

        let mut leaves: LinkedList<Box<MerkleTreeNode>> = transactions
            .iter()
            .map(|tx| {
                Box::new(MerkleTreeNode::new(
                    None,
                    None,
                    tx.get_hash_for_signature(),
                    true,
                    1 as usize,
                ))
            })
            .collect();

        while leaves.len() > 1 {
            let mut nodes: LinkedList<MerkleTreeNode> = Default::default();

            // Create a node per two leaves
            while !leaves.is_empty() {
                let left = leaves.pop_front();
                let right = leaves.pop_front(); //Can be None, this is expected
                let count = MerkleTree::calculate_child_count(&left, &right);
                nodes.push_back(MerkleTreeNode::new(left, right, None, false, count));
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

    pub fn traverse(&self, read_func: impl Fn(&MerkleTreeNode)) {
        MerkleTree::traverse_node(&self.root, &read_func);
    }

    pub fn create_clone(&self) -> Box<MerkleTree> {
        return Box::new(MerkleTree {
            root: MerkleTree::clone_node(Some(&self.root)).unwrap(),
        });
    }

    pub fn prune(&mut self, prune_func: impl Fn(&SaitoHash) -> bool) {
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

        let mut vbytes: Vec<u8> = vec![];

        match node.left.as_ref() {
            Some(left) => {
                vbytes.extend(left.hash.unwrap());
            }
            None => {
                vbytes.extend(&[0u8; 32]);
            }
        }

        match node.right.as_ref() {
            Some(right) => {
                vbytes.extend(right.hash.unwrap());
            }
            None => {
                vbytes.extend(&[0u8; 32]);
            }
        }

        node.hash = Some(hash(&vbytes));

        return true;
    }

    fn traverse_node(node: &MerkleTreeNode, read_func: &impl Fn(&MerkleTreeNode)) {
        if node.left.is_some() {
            MerkleTree::traverse_node(&node.left.as_ref().unwrap(), read_func);
        }

        if node.right.is_some() {
            MerkleTree::traverse_node(&node.right.as_ref().unwrap(), read_func);
        }

        read_func(node);
    }

    fn clone_node(node: Option<&Box<MerkleTreeNode>>) -> Option<Box<MerkleTreeNode>> {
        match node {
            None => None,
            Some(source) => Some(Box::new(MerkleTreeNode::new(
                MerkleTree::clone_node(source.left.as_ref()),
                MerkleTree::clone_node(source.right.as_ref()),
                source.hash,
                source.transaction,
                source.count,
            ))),
        }
    }

    fn prune_node(
        node: Option<&mut Box<MerkleTreeNode>>,
        prune_func: &impl Fn(&SaitoHash) -> bool,
    ) -> bool {
        return if node.is_some() {
            let mut node = node.unwrap();
            if node.transaction {
                prune_func(&node.hash.unwrap())
            } else {
                let mut prune = true;

                if node.left.is_some() {
                    prune &= MerkleTree::prune_node(node.left.as_mut(), prune_func);
                }

                if node.right.is_some() {
                    prune &= MerkleTree::prune_node(node.right.as_mut(), prune_func);
                }

                if prune {
                    node.left = None;
                    node.right = None;
                    node.count = 1;
                } else {
                    node.count = MerkleTree::calculate_child_count(&node.left, &node.right);
                }

                prune
            }
        } else {
            true
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::common::defs::SaitoHash;
    use crate::core::data::crypto::hash;
    use crate::core::data::merkle::{MerkleTree, MerkleTreeNode};
    use crate::core::data::transaction::Transaction;
    use crate::core::data::wallet::Wallet;
    use log::{debug, trace};
    use std::collections::LinkedList;

    #[test]
    fn merkle_tree_generation_test() {
        let wallet = Wallet::new();

        let mut transactions = vec![];

        for i in 0..5 {
            let mut transaction = Transaction::new();
            transaction.set_timestamp(i);
            transaction.sign(wallet.get_privatekey());
            transactions.push(transaction);
        }

        let tree1 = MerkleTree::generate(&transactions).unwrap();

        transactions[0].set_timestamp(10);
        transactions[0].sign(wallet.get_privatekey());
        let tree2 = MerkleTree::generate(&transactions).unwrap();

        transactions[4].set_timestamp(11);
        transactions[4].sign(wallet.get_privatekey());
        let tree3 = MerkleTree::generate(&transactions).unwrap();

        transactions[2].set_timestamp(12);
        transactions[2].sign(wallet.get_privatekey());
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
            transaction.set_timestamp(i);
            transaction.sign(wallet.get_privatekey());
            transactions.push(transaction);
        }

        let target_hash = transactions[0].get_hash_for_signature().unwrap();

        let tree = MerkleTree::generate(&transactions).unwrap();
        let cloned_tree = tree.create_clone();
        let mut pruned_tree = tree.create_clone();
        pruned_tree.prune(|hash| target_hash != *hash);

        assert_eq!(tree.get_root_hash(), cloned_tree.get_root_hash());
        assert_eq!(cloned_tree.get_root_hash(), pruned_tree.get_root_hash());
        assert_eq!(tree.len(), 11);
        assert_eq!(cloned_tree.len(), tree.len());
        assert_eq!(pruned_tree.len(), 7);

        println!("\ntree");
        tree.traverse(|node| print!("{}, ", hex::encode(node.hash.unwrap())));

        println!("\ncloned_tree");
        cloned_tree.traverse(|node| print!("{}, ", hex::encode(node.hash.unwrap())));

        println!("\npruned_tree");
        pruned_tree.traverse(|node| print!("{}, ", hex::encode(node.hash.unwrap())));
    }
}

use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
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
    prune: bool,
    count: usize,
}

impl MerkleTreeNode {
    pub fn new(
        left: Option<Box<MerkleTreeNode>>,
        right: Option<Box<MerkleTreeNode>>,
        hash: Option<SaitoHash>,
        prune: bool,
    ) -> MerkleTreeNode {
        MerkleTreeNode {
            left,
            right,
            hash,
            prune,
            count: 1 as usize,
        }
    }

    fn generate(&mut self) -> bool {
        if self.hash.is_some() {
            return true;
        }

        let mut vbytes: Vec<u8> = vec![];

        match self.left.as_ref() {
            Some(node) => {
                vbytes.extend(node.hash.unwrap());
                self.prune &= node.prune;
                self.count += node.count;
            }
            None => {
                vbytes.extend(&[0u8; 32]);
            }
        }

        match self.right.as_ref() {
            Some(node) => {
                vbytes.extend(node.hash.unwrap());
                self.prune &= node.prune;
                self.count += node.count;
            }
            None => {
                vbytes.extend(&[0u8; 32]);
            }
        }

        self.hash = Some(hash(&vbytes));

        if self.prune {
            self.left = None;
            self.right = None;
            self.count = 1 as usize;
        }

        return true;
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
        return MerkleTree::generate_with_pruning(transactions, |tx| false);
    }

    pub fn generate_with_pruning(
        transactions: &Vec<Transaction>,
        prune_func: fn(&Transaction) -> bool,
    ) -> Option<Box<MerkleTree>> {
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
                    prune_func(tx),
                ))
            })
            .collect();

        while leaves.len() > 1 {
            let mut nodes: LinkedList<MerkleTreeNode> = Default::default();

            // Create a node per two leaves
            while !leaves.is_empty() {
                nodes.push_back(MerkleTreeNode::new(
                    leaves.pop_front(),
                    leaves.pop_front(),
                    None,
                    true,
                ));
            }

            // Compute the node hashes in parallel
            nodes.par_iter_mut().all(|node| node.generate());
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
}

#[cfg(test)]
mod tests {
    use crate::common::defs::SaitoHash;
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

        let root1 = MerkleTree::generate(&transactions).unwrap().get_root_hash();

        transactions[0].set_timestamp(10);
        transactions[0].sign(wallet.get_privatekey());
        let root2 = MerkleTree::generate(&transactions).unwrap().get_root_hash();

        transactions[4].set_timestamp(11);
        transactions[4].sign(wallet.get_privatekey());
        let root3 = MerkleTree::generate(&transactions).unwrap().get_root_hash();

        transactions[2].set_timestamp(12);
        transactions[2].sign(wallet.get_privatekey());
        let tree1 = MerkleTree::generate_with_pruning(&transactions, |tx| false).unwrap();
        let root4 = tree1.get_root_hash();
        let tree2 = MerkleTree::generate_with_pruning(&transactions, |tx| true).unwrap();
        let root5 = tree2.get_root_hash();

        transactions[0].set_timestamp(100);
        transactions[0].sign(wallet.get_privatekey());
        let tree3 =
            MerkleTree::generate_with_pruning(&transactions, |tx| tx.get_timestamp() != 100)
                .unwrap();
        let root6 = tree2.get_root_hash();

        assert_eq!(root1.len(), 32);
        assert_ne!(root1, [0; 32]);

        assert_eq!(root2.len(), 32);
        assert_ne!(root2, [0; 32]);

        assert_eq!(root3.len(), 32);
        assert_ne!(root3, [0; 32]);

        assert_eq!(root4.len(), 32);
        assert_ne!(root4, [0; 32]);

        assert_ne!(root1, root2);
        assert_ne!(root2, root3);
        assert_ne!(root3, root4);

        assert_eq!(root4, root5);
        assert_eq!(root5, root6);
        assert_eq!(tree1.len(), 11);
        assert_eq!(tree2.len(), 1);
        assert_eq!(tree3.len(), 7);
    }
}

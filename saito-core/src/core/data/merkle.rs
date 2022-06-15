use std::collections::LinkedList;
use ahash::{AHashMap, AHashSet};
use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
use rayon::prelude::*;

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
}

impl MerkleTreeNode {
    pub fn new(left: Option<Box<MerkleTreeNode>>, right: Option<Box<MerkleTreeNode>>, hash : Option<SaitoHash>) -> MerkleTreeNode {
        MerkleTreeNode {
            left,
            right,
            hash,
        }
    }

     fn generate_hash(&mut self) -> bool {

         if self.hash.is_some() {
             return true;
         }

         let mut vbytes: Vec<u8> = vec![];
         if self.left.is_some() {
             vbytes.extend(self.left.as_ref().unwrap().get_hash());
         } else {
             vbytes.extend(&[0u8;32]);
         }

         if self.right.is_some() {
             vbytes.extend(self.right.as_ref().unwrap().get_hash());
         } else {
             vbytes.extend(&[0u8;32]);
         }

         self.hash = Some(hash(&vbytes));
         return true;
    }

    fn get_hash(&self) -> SaitoHash {
        self.hash.unwrap()
    }
}

pub struct MerkleTree {
    root : Option<Box<MerkleTreeNode>>,
}

impl MerkleTree {
    pub fn get_root_hash(&self) -> SaitoHash {
        return self.get_root().get_hash();
    }

    pub fn get_root(&self) -> &Box<MerkleTreeNode> {
        return &self.root.as_ref().unwrap();
    }

    pub fn generate(transactions: &Vec<Transaction>) -> Box<MerkleTree> {

        // Logic: a single node will be created per two leaves, each node will generate
        // a hash by combining the hashes of the two leaves, in the next loop
        // those node hashes will used as the leaves for the next set of nodes,
        // i.e. leaf count is reduced by half on each loop, eventually ending up in 1
        let mut tree = Box::new(MerkleTree{root : None, });

        let mut leaves : LinkedList<Box<MerkleTreeNode>> =
            transactions.iter().map(|tx| {
                let node = Box::new(
                    MerkleTreeNode::new(None, None,tx.get_hash_for_signature()));
                node
            }).collect();

        let mut nodes: LinkedList<Box<MerkleTreeNode>> = Default::default();

        while leaves.len() > 1 {

            // Create a node per two leaves
            nodes.clear();

            while !leaves.is_empty() {
                nodes.push_back(Box::new(MerkleTreeNode::new(leaves.pop_front(), leaves.pop_front(), None)));
            }

            // Compute the node hashes in parallel
            nodes.par_iter_mut().all(|node| node.generate_hash());
            // Collect the next set of leaves for the computation
            leaves.clear();

            while !nodes.is_empty() {
                let node = nodes.pop_front().unwrap();
                leaves.push_back(node);
            }
        }

        tree.root = leaves.pop_front();
        return tree;
    }

    pub fn generate_root(transactions: &Vec<Transaction>) -> SaitoHash {
        MerkleTree::generate(transactions).get_root_hash()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::LinkedList;
    use ahash::AHashSet;
    use log::{debug, trace};
    use crate::common::defs::SaitoHash;
    use crate::core::data::merkle::{MerkleTree, MerkleTreeNode};
    use crate::core::data::transaction::Transaction;
    use crate::core::data::wallet::Wallet;

    #[test]
    fn merkle_tree_generation_test() {
        let wallet = Wallet::new();

        let mut transactions = (0..5)
            .into_iter()
            .map(|_| {
                let mut transaction = Transaction::new();
                transaction.sign(wallet.get_privatekey());
                transaction
            })
            .collect();

        let root1 = MerkleTree::generate(&transactions).get_root_hash();

        transactions[0].set_timestamp(1);
        transactions[0].sign(wallet.get_privatekey());
        let root2 = MerkleTree::generate(&transactions).get_root_hash();

        transactions[4].set_timestamp(1);
        transactions[4].sign(wallet.get_privatekey());
        let root3 = MerkleTree::generate(&transactions).get_root_hash();

        transactions[2].set_timestamp(1);
        transactions[2].sign(wallet.get_privatekey());
        let root4 = MerkleTree::generate(&transactions).get_root_hash();
        let tree = MerkleTree::generate(&transactions);
        let root5 = tree.get_root_hash();

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

        let mut hash_set: AHashSet<SaitoHash> = Default::default();

        transactions.iter().for_each(|tx|{let _ = hash_set.insert(tx.get_hash_for_signature().unwrap());});

        let mut node_list: LinkedList<&Box<MerkleTreeNode>> = Default::default();;
        node_list.push_back(tree.get_root());

        let mut count = 0;

        while !node_list.is_empty() {
            count += 1;
            let node = node_list.pop_front().unwrap();
            println!("found node {:?}: {:?}", count, hex::encode(node.get_hash()));
            hash_set.insert(node.get_hash());

            if node.left.is_some() {
                node_list.push_back(node.left.as_ref().unwrap());
            }

            if node.right.is_some() {
                node_list.push_back(node.right.as_ref().unwrap());
            }
        }

       transactions.iter().for_each(|tx|assert!(hash_set.contains(&tx.get_hash_for_signature().unwrap())));
    }
}

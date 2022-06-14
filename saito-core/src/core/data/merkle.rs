use crate::common::defs::SaitoHash;
use crate::core::data::crypto::hash;
use crate::core::data::transaction::Transaction;
use rayon::prelude::*;

//
// MerkleTreeLayer is a short implementation that uses the default
// Saito hashing algorithm. It is also written to take advantage of
// rayon parallelization.
//
#[derive(PartialEq, Debug, Clone)]
pub struct MerkleTreeNode {
    left: SaitoHash,
    right: SaitoHash,
    hash: SaitoHash,
}

impl MerkleTreeNode {
    pub fn new(left: SaitoHash, right: SaitoHash) -> MerkleTreeNode {
        MerkleTreeNode {
            left,
            right,
            hash: [0; 32],
        }
    }

    pub fn generate_hash(&mut self) -> bool {
        let mut vbytes: Vec<u8> = vec![];
        vbytes.extend(&self.left);
        vbytes.extend(&self.right);
        self.hash = hash(&vbytes);
        true
    }

    pub fn get_hash(&self) -> SaitoHash {
        self.hash
    }
}

pub struct MerkleTree {}

impl MerkleTree {
    pub fn generate_root(transactions: &Vec<Transaction>) -> SaitoHash {
        if transactions.is_empty() {
            return [0; 32];
        }

        // Logic: a single node will be created per two leaves, each node will generate
        // a hash by combining the hashes of the two leaves, in the next loop
        // those node hashes will used as the leaves for the next set of nodes,
        // i.e. leaf count is reduced by half on each loop, eventually ending up in 1

        let mut leaves: Vec<SaitoHash> = transactions
            .iter()
            .map(|tx| tx.get_hash_for_signature().unwrap())
            .collect();

        let mut nodes: Vec<MerkleTreeNode> = vec![];

        while leaves.len() > 1 {
            // Create a node per two leaves
            nodes.clear();
            for i in (0..leaves.len()).step_by(2) {
                nodes.push(MerkleTreeNode::new(
                    leaves[i],
                    if (i + 1) < leaves.len() {
                        leaves[i + 1]
                    } else {
                        [0; 32]
                    },
                ));
            }

            // Compute the node hashes in parallel
            nodes.par_iter_mut().all(|node| node.generate_hash());
            // Collect the next set of leaves for the computation
            leaves.clear();
            nodes.iter().for_each(|leaf| leaves.push(leaf.get_hash()));
        }

        return leaves[0];
    }
}

#[cfg(test)]
mod tests {
    use crate::core::data::merkle::MerkleTree;
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

        let root1 = MerkleTree::generate_root(&transactions);

        transactions[0].set_timestamp(1);
        transactions[0].sign(wallet.get_privatekey());
        let root2 = MerkleTree::generate_root(&transactions);

        transactions[4].set_timestamp(1);
        transactions[4].sign(wallet.get_privatekey());
        let root3 = MerkleTree::generate_root(&transactions);

        transactions[2].set_timestamp(1);
        transactions[2].sign(wallet.get_privatekey());
        let root4 = MerkleTree::generate_root(&transactions);
        let root5 = MerkleTree::generate_root(&transactions);

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
    }
}

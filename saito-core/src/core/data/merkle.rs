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
pub struct MerkleTreeLayer {
    left: SaitoHash,
    right: SaitoHash,
    hash: SaitoHash,
    level: u8,
}

impl MerkleTreeLayer {
    pub fn new(left: SaitoHash, right: SaitoHash, level: u8) -> MerkleTreeLayer {
        MerkleTreeLayer {
            left,
            right,
            level,
            hash: [0; 32],
        }
    }

    pub fn hash(&mut self) -> bool {
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

        let tx_sig_hashes: Vec<SaitoHash> = transactions
            .iter()
            .map(|tx| tx.get_hash_for_signature().unwrap())
            .collect();

        let mut mrv: Vec<MerkleTreeLayer> = vec![];

        //
        // or let's try another approach
        //
        let tsh_len = tx_sig_hashes.len();
        let mut leaf_depth = 0;

        for i in 0..tsh_len {
            if (i + 1) < tsh_len {
                mrv.push(MerkleTreeLayer::new(
                    tx_sig_hashes[i],
                    tx_sig_hashes[i + 1],
                    leaf_depth,
                ));
            } else {
                mrv.push(MerkleTreeLayer::new(tx_sig_hashes[i], [0; 32], leaf_depth));
            }
        }

        let mut start_point = 0;
        let mut stop_point = mrv.len();
        let mut keep_looping = true;

        while keep_looping {
            // processing new layer
            leaf_depth += 1;

            // hash the parent in parallel
            mrv[start_point..stop_point]
                .par_iter_mut()
                .all(|leaf| leaf.hash());

            let start_point_old = start_point;
            start_point = mrv.len();

            for i in (start_point_old..stop_point).step_by(2) {
                if (i + 1) < stop_point {
                    mrv.push(MerkleTreeLayer::new(
                        mrv[i].get_hash(),
                        mrv[i + 1].get_hash(),
                        leaf_depth,
                    ));
                } else {
                    mrv.push(MerkleTreeLayer::new(mrv[i].get_hash(), [0; 32], leaf_depth));
                }
            }

            stop_point = mrv.len();
            if stop_point > 0 {
                keep_looping = start_point < stop_point - 1;
            } else {
                keep_looping = false;
            }
        }

        //
        // hash the final leaf
        //
        mrv[start_point].hash();
        return mrv[start_point].get_hash();
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

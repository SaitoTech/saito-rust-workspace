#[cfg(test)]
pub mod test {
    use crate::core::consensus::block::Block;
    use crate::core::defs::Currency;
    use crate::core::util::test::node_tester::test::NodeTester;

    pub struct BlockTester {
        pub block: Block,
    }

    impl Default for BlockTester {
        fn default() -> Self {
            BlockTester {
                block: Block::new(),
            }
        }
    }
    impl BlockTester {
        pub async fn new(&mut self, node_tester: NodeTester) {}
        pub async fn add_transaction(&mut self, with_payment: Currency, with_fee: Currency) {}
    }
}

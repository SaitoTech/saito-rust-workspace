use criterion::{black_box, criterion_group, criterion_main, Criterion};

mod benchmarks;

criterion_main! {
    benchmarks::hashing::hashing_group,
    benchmarks::serialize_tx::serializing_tx_group,
    benchmarks::serialize_block::serializing_block_group
}

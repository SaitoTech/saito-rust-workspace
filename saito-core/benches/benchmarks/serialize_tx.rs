use criterion::{black_box, criterion_group, criterion_main, Criterion};
use saito_core::core::data::transaction::Transaction;

pub fn serialize_tx(c: &mut Criterion) {
    let tx = Transaction::new();
    c.bench_function("serializing tx with 0 slips and empty buffer", |b| {
        b.iter(|| {
            tx.serialize_for_net();
        });
    });
}
criterion_group!(serializing_tx_group, serialize_tx);

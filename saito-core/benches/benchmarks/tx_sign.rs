use criterion::{black_box, criterion_group, Criterion};
use hex::FromHex;

use saito_core::core::data::slip::Slip;
use saito_core::core::data::transaction::Transaction;

fn generate_tx(input_slip_count: u64, output_slip_count: u64, buffer_size: u64) -> Transaction {
    let mut tx = Transaction::new();

    for _ in 0..input_slip_count {
        let slip = Slip::new();
        tx.inputs.push(slip);
    }

    for _ in 0..output_slip_count {
        let slip = Slip::new();
        tx.outputs.push(slip);
    }

    tx.message = vec![1; buffer_size as usize];
    tx
}

pub fn tx_sign(c: &mut Criterion) {
    let private_key =
        <[u8; 32]>::from_hex("854702489d49c7fb2334005b903580c7a48fe81121ff16ee6d1a528ad32f235d")
            .unwrap();
    let mut tx = generate_tx(0, 0, 0);
    c.bench_function("signing tx with 0 slips and empty buffer", |b| {
        b.iter(|| {
            black_box(tx.sign(&private_key));
        });
    });
    let mut tx = generate_tx(100, 100, 1000000);
    c.bench_function("signing tx with 100 slips and 1MB buffer", |b| {
        b.iter(|| {
            black_box(tx.sign(&private_key));
        });
    });
}

criterion_group!(tx_sign_group, tx_sign);

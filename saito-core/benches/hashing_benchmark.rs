use blake3::Hasher;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use saito_core::core::data::crypto::hash;

fn sort(arr: &mut Vec<u8>) {
    arr.sort()
}

fn hash_fn_serial(buffers: &Vec<Vec<u8>>) {
    let mut hasher = Hasher::new();
    for buffer in buffers {
        hasher.update(buffer);
        hasher.finalize();
    }
}

fn hash_fn_parallel(buffers: &Vec<Vec<u8>>) {
    let mut hasher = Hasher::new();
    for buffer in buffers {
        hasher.update_rayon(buffer);
        hasher.finalize();
    }
}

fn hashing_benchmark(c: &mut Criterion) {
    let mut buffers = vec![];
    for i in 1..1000 {
        let buffer: Vec<u8> = vec![0; i * 1000];
        buffers.push(buffer);
    }
    c.bench_function("hash serial", |b| b.iter(|| hash_fn_serial(&buffers)));
    c.bench_function("hash parallel", |b| b.iter(|| hash_fn_parallel(&buffers)));
}
criterion_group!(benches, hashing_benchmark);
criterion_main!(benches);

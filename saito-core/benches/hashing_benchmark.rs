use blake3::Hasher;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

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

fn hashing_general(c: &mut Criterion) {
    let mut buffers = vec![];
    for i in 1..1000 {
        let buffer: Vec<u8> = vec![0; i * 1000];
        buffers.push(buffer);
    }
    c.bench_function("hash serial", |b| b.iter(|| hash_fn_serial(&buffers)));
    c.bench_function("hash parallel", |b| b.iter(|| hash_fn_parallel(&buffers)));

    let mut buffers = vec![];
    for i in 1..1000 {
        let buffer: Vec<u8> = vec![0; 1000];
        buffers.push(buffer);
    }
    c.bench_function("hash 1K serial", |b| b.iter(|| hash_fn_serial(&buffers)));
    c.bench_function("hash 1K parallel", |b| {
        b.iter(|| hash_fn_parallel(&buffers))
    });

    let mut buffers = vec![];
    for i in 1..1000 {
        let buffer: Vec<u8> = vec![0; 1_000_000];
        buffers.push(buffer);
    }
    c.bench_function("hash 1M serial", |b| b.iter(|| hash_fn_serial(&buffers)));
    c.bench_function("hash 1M parallel", |b| {
        b.iter(|| hash_fn_parallel(&buffers))
    });
}
criterion_group!(hashing, hashing_general);
criterion_main!(hashing);

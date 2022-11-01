use criterion::{criterion_group, Criterion};

fn extend() {
    let buffer = [0; 33];
    let buffer2 = [0; 32];
    let mut v = vec![];
    v.extend(buffer);
    v.extend(buffer2);
    assert!(v.len() > 0);
}

fn append() {
    let mut buffer = vec![0; 33];
    let mut buffer2 = vec![0; 32];
    let mut v = vec![];
    v.append(&mut buffer);
    v.append(&mut buffer2);
    assert!(v.len() > 0);
}

fn concat() {
    let buffer = [0; 33];
    let buffer2 = [0; 32];
    let buffer = [buffer.as_slice(), buffer2.as_slice()].concat();
    assert!(buffer.len() > 0);
}

fn join() {
    let buffer = [0; 33];
    let buf2 = [0; 32];

    let buffer = [buffer.as_slice(), buf2.as_slice()].join(&0);
    assert!(buffer.len() > 0);
}

pub fn misc(c: &mut Criterion) {
    c.bench_function("extend", |b| b.iter(|| extend()));
    c.bench_function("append", |b| b.iter(|| append()));
    c.bench_function("concat", |b| b.iter(|| concat()));
    c.bench_function("join", |b| b.iter(|| join()));
}

criterion_group!(misc_group, misc);

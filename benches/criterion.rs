mod insert;
mod rand_access;

use criterion::criterion_main;

criterion_main!(insert::bench_insert, rand_access::bench_rand_access);

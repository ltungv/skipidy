use std::collections::BTreeSet;

use criterion::{AxisScale, BenchmarkId, Criterion, PlotConfiguration, criterion_group};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use skipidy::SkipList;

const SIZES: [usize; 6] = [1, 10, 100, 1000, 10_000, 100_000];

pub fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for size in SIZES {
        group.bench_function(BenchmarkId::new("BTreeSet", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut bt: BTreeSet<u64> = BTreeSet::new();
            for _ in 0..size {
                bt.insert(rng.random());
            }
            b.iter(|| {
                bt.insert(rng.random());
            });
        });
        group.bench_function(BenchmarkId::new("SkipList", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl: SkipList<u64, _, 32> = SkipList::new();
            for _ in 0..size {
                sl.insert(rng.random());
            }
            b.iter(|| {
                sl.insert(rng.random());
            });
        });
        group.bench_function(BenchmarkId::new("skiplist::OrderedSkipList", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl = skiplist::OrderedSkipList::<u64>::new();
            for _ in 0..size {
                sl.insert(rng.random());
            }
            b.iter(|| {
                sl.insert(rng.random());
            });
        });
        group.bench_function(BenchmarkId::new("skiplist::SkipMap", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl = skiplist::SkipMap::<u64, ()>::new();
            for _ in 0..size {
                sl.insert(rng.random(), ());
            }
            b.iter(|| {
                sl.insert(rng.random(), ());
            });
        });
    }
}

criterion_group!(bench_insert, bench);

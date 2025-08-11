use std::{collections::BTreeSet, hint::black_box};

use criterion::{AxisScale, BenchmarkId, Criterion, PlotConfiguration, criterion_group};
use rand::{Rng, SeedableRng, rngs::SmallRng, seq::IndexedRandom};
use skipidy::SkipList;

const ACCESSES: usize = 10;
const SIZES: [usize; 6] = [1, 10, 100, 1000, 10_000, 100_000];

pub fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("rand_access");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for size in SIZES {
        group.bench_function(BenchmarkId::new("BTreeSet", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut bt: BTreeSet<u64> = BTreeSet::new();
            let items: Vec<u64> = std::iter::repeat_with(|| rng.random()).take(size).collect();
            let access: Vec<_> = items.choose_multiple(&mut rng, ACCESSES).collect();
            for item in &items {
                bt.insert(*item);
            }
            b.iter(|| {
                for val in &access {
                    black_box(bt.contains(val));
                }
            });
        });
        group.bench_function(BenchmarkId::new("SkipList", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl: SkipList<u64, _, 32> = SkipList::new();
            let items: Vec<u64> = std::iter::repeat_with(|| rng.random()).take(size).collect();
            let access: Vec<_> = items.choose_multiple(&mut rng, ACCESSES).collect();
            for item in &items {
                sl.insert(*item);
            }
            b.iter(|| {
                for val in &access {
                    black_box(sl.contains(val));
                }
            });
        });
        group.bench_function(BenchmarkId::new("skiplist::OrderedSkipList", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl = skiplist::OrderedSkipList::<u64>::new();
            let items: Vec<u64> = std::iter::repeat_with(|| rng.random()).take(size).collect();
            let access: Vec<_> = items.choose_multiple(&mut rng, ACCESSES).collect();
            for item in &items {
                sl.insert(*item);
            }
            b.iter(|| {
                for val in &access {
                    black_box(sl.contains(val));
                }
            });
        });
        group.bench_function(BenchmarkId::new("skiplist::SkipMap", size), |b| {
            let mut rng = SmallRng::seed_from_u64(0x1234_abcd);
            let mut sl = skiplist::SkipMap::<u64, ()>::new();
            let items: Vec<u64> = std::iter::repeat_with(|| rng.random()).take(size).collect();
            let access: Vec<_> = items.choose_multiple(&mut rng, ACCESSES).collect();
            for item in &items {
                sl.insert(*item, ());
            }
            b.iter(|| {
                for val in &access {
                    black_box(sl.contains_key(val));
                }
            });
        });
    }
}

criterion_group!(bench_rand_access, bench);

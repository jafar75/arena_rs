use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use arena_rs::Arena;

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

const COUNTS: &[usize] = &[100, 1_000, 10_000, 100_000];

// ── Single allocation ────────────────────────────────────────────────────────

fn bench_single_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_alloc");

    group.bench_function("arena", |b| {
        let mut arena = Arena::new(1024).unwrap();
        b.iter(|| {
            arena.reset();
            let p = arena.alloc(black_box(Point { x: 1.0, y: 2.0 })).unwrap();
            black_box(p);
        });
    });

    group.bench_function("box", |b| {
        b.iter(|| {
            let p = Box::new(black_box(Point { x: 1.0, y: 2.0 }));
            black_box(p);
        });
    });

    group.finish();
}

// ── Bulk allocation ──────────────────────────────────────────────────────────

fn bench_bulk_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_alloc");

    for &count in COUNTS {
        let arena_size = count * size_of::<Point>();

        group.bench_with_input(BenchmarkId::new("arena", count), &count, |b, &count| {
            let mut arena = Arena::new(arena_size).unwrap();
            b.iter(|| {
                arena.reset();
                let slice = arena
                    .alloc_array(count, |i| Point { x: i as f64, y: i as f64 * 2.0 })
                    .unwrap();
                black_box(slice);
            });
        });

        group.bench_with_input(BenchmarkId::new("vec", count), &count, |b, &count| {
            b.iter(|| {
                let mut v = Vec::with_capacity(count);
                for i in 0..count {
                    v.push(Point { x: i as f64, y: i as f64 * 2.0 });
                }
                black_box(v);
            });
        });
    }

    group.finish();
}

// ── Reset + reuse cycle ──────────────────────────────────────────────────────

fn bench_reset_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("reset_reuse");
    const COUNT: usize = 1_000;
    let arena_size = COUNT * size_of::<Point>();

    // Arena: allocate, reset, repeat — no heap traffic after the first alloc
    group.bench_function("arena", |b| {
        let mut arena = Arena::new(arena_size).unwrap();
        b.iter(|| {
            arena.reset();
            let slice = arena
                .alloc_array(COUNT, |i| Point { x: i as f64, y: 0.0 })
                .unwrap();
            black_box(slice);
        });
    });

    // Vec: drop + reallocate every iteration
    group.bench_function("vec_drop_alloc", |b| {
        b.iter(|| {
            let v: Vec<Point> = (0..COUNT).map(|i| Point { x: i as f64, y: 0.0 }).collect();
            black_box(v);
            // v dropped here, heap freed
        });
    });

    group.finish();
}

criterion_group!(benches, bench_single_alloc, bench_bulk_alloc, bench_reset_reuse);
criterion_main!(benches);
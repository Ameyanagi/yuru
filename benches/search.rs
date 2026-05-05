use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yomi_core::{build_index, search, PlainBackend, SearchConfig};
use yomi_ja::JapaneseBackend;
use yomi_zh::ChineseBackend;

fn bench_plain_search(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..10_000).map(|idx| format!("src/module_{idx}/README.md"));
    let index = build_index(candidates, &PlainBackend, &cfg);

    c.bench_function("plain_search_10k_read", |b| {
        b.iter(|| {
            search(
                black_box("read"),
                black_box(&index),
                &PlainBackend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_plain_search_100k(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..100_000).map(|idx| format!("src/module_{idx}/README.md"));
    let index = build_index(candidates, &PlainBackend, &cfg);

    c.bench_function("plain_search_100k_read", |b| {
        b.iter(|| {
            search(
                black_box("read"),
                black_box(&index),
                &PlainBackend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_ja_search(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..10_000).map(|idx| {
        if idx % 100 == 0 {
            format!("カメラ_{idx}.txt")
        } else {
            format!("notes/{idx}.txt")
        }
    });
    let backend = JapaneseBackend;
    let index = build_index(candidates, &backend, &cfg);

    c.bench_function("ja_search_10k_kamera", |b| {
        b.iter(|| {
            search(
                black_box("kamera"),
                black_box(&index),
                &backend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_ja_search_100k(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..100_000).map(|idx| {
        if idx % 100 == 0 {
            format!("カメラ_{idx}.txt")
        } else {
            format!("notes/{idx}.txt")
        }
    });
    let backend = JapaneseBackend;
    let index = build_index(candidates, &backend, &cfg);

    c.bench_function("ja_search_100k_kamera", |b| {
        b.iter(|| {
            search(
                black_box("kamera"),
                black_box(&index),
                &backend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_zh_search(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..10_000).map(|idx| {
        if idx % 100 == 0 {
            format!("北京大学_{idx}.txt")
        } else {
            format!("docs/{idx}.txt")
        }
    });
    let backend = ChineseBackend;
    let index = build_index(candidates, &backend, &cfg);

    c.bench_function("zh_search_10k_bjdx", |b| {
        b.iter(|| {
            search(
                black_box("bjdx"),
                black_box(&index),
                &backend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_zh_search_100k(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates = (0..100_000).map(|idx| {
        if idx % 100 == 0 {
            format!("北京大学_{idx}.txt")
        } else {
            format!("docs/{idx}.txt")
        }
    });
    let backend = ChineseBackend;
    let index = build_index(candidates, &backend, &cfg);

    c.bench_function("zh_search_100k_bjdx", |b| {
        b.iter(|| {
            search(
                black_box("bjdx"),
                black_box(&index),
                &backend,
                black_box(&cfg),
            )
        });
    });
}

fn bench_plain_search_1m(c: &mut Criterion) {
    if std::env::var("YOMI_BENCH_1M").as_deref() != Ok("1") {
        return;
    }

    let cfg = SearchConfig::default();
    let candidates = (0..1_000_000).map(|idx| format!("src/module_{idx}/README.md"));
    let index = build_index(candidates, &PlainBackend, &cfg);

    let mut group = c.benchmark_group("large");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("plain_search_1m_read_opt_in", |b| {
        b.iter(|| {
            search(
                black_box("read"),
                black_box(&index),
                &PlainBackend,
                black_box(&cfg),
            )
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_plain_search,
    bench_plain_search_100k,
    bench_ja_search,
    bench_ja_search_100k,
    bench_zh_search,
    bench_zh_search_100k,
    bench_plain_search_1m
);
criterion_main!(benches);

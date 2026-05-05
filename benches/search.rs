use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use yuru_core::{build_index, search, PlainBackend, SearchConfig};
use yuru_ja::JapaneseBackend;
use yuru_zh::ChineseBackend;

fn bench_build_index_plain_100k(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates: Vec<_> = (0..100_000)
        .map(|idx| format!("src/module_{idx}/README.md"))
        .collect();

    c.bench_function("plain_build_index_100k", |b| {
        b.iter_batched(
            || candidates.clone(),
            |items| build_index(black_box(items), &PlainBackend, black_box(&cfg)),
            BatchSize::LargeInput,
        );
    });
}

fn bench_build_index_ja_100k(c: &mut Criterion) {
    let cfg = SearchConfig::default();
    let candidates: Vec<_> = (0..100_000)
        .map(|idx| {
            if idx % 100 == 0 {
                format!("カメラ_{idx}.txt")
            } else {
                format!("notes/{idx}.txt")
            }
        })
        .collect();
    let backend = JapaneseBackend::default();

    c.bench_function("ja_build_index_100k", |b| {
        b.iter_batched(
            || candidates.clone(),
            |items| build_index(black_box(items), &backend, black_box(&cfg)),
            BatchSize::LargeInput,
        );
    });
}

fn kanji_heavy_worst_candidates(count: usize) -> Vec<String> {
    const TERMS: &[&str] = &[
        "日本語形態素解析結果確認依頼重要資料",
        "東京都新宿区西新宿再開発計画審査記録",
        "大阪大学情報科学研究科共同研究報告書",
        "京都駅周辺観光案内更新履歴管理台帳",
        "横浜港国際物流統計輸出入分析資料",
        "北海道札幌市気象観測長期予測比較表",
        "福岡市地下鉄運行障害復旧作業記録",
        "名古屋城文化財保存修復調査議事録",
        "神戸市中央区医療機関連携会議資料",
        "沖縄県那覇空港国際線利用状況集計",
    ];

    (0..count)
        .map(|idx| {
            let a = TERMS[idx % TERMS.len()];
            let b = TERMS[(idx / TERMS.len() + 3) % TERMS.len()];
            let c = TERMS[(idx / 97 + 7) % TERMS.len()];
            format!("資料/{a}/{b}/{c}/令和六年度第{idx:06}号追加調査結果最終確認版.txt")
        })
        .collect()
}

fn bench_ja_kanji_heavy(c: &mut Criterion) {
    if std::env::var("YURU_BENCH_KANJI_HEAVY").as_deref() != Ok("1") {
        return;
    }

    let cfg = SearchConfig::default();
    let backend = JapaneseBackend::default();

    let build_10k = kanji_heavy_worst_candidates(10_000);
    let search_100k = kanji_heavy_worst_candidates(100_000);
    let index_100k = build_index(search_100k, &backend, &cfg);

    let mut group = c.benchmark_group("kanji_heavy");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("worst_ja_build_index_10k", |b| {
        b.iter_batched(
            || build_10k.clone(),
            |items| build_index(black_box(items), &backend, black_box(&cfg)),
            BatchSize::LargeInput,
        );
    });
    group.bench_function("worst_ja_search_100k_hit", |b| {
        b.iter(|| {
            search(
                black_box("nihongo"),
                black_box(&index_100k),
                &backend,
                black_box(&cfg),
            )
        });
    });
    group.bench_function("worst_ja_search_100k_nohit", |b| {
        b.iter(|| {
            search(
                black_box("zzzzzzzz"),
                black_box(&index_100k),
                &backend,
                black_box(&cfg),
            )
        });
    });
    group.finish();
}

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
    let backend = JapaneseBackend::default();
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
    let backend = JapaneseBackend::default();
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
    let backend = ChineseBackend::default();
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
    let backend = ChineseBackend::default();
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
    if std::env::var("YURU_BENCH_1M").as_deref() != Ok("1") {
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
    bench_build_index_plain_100k,
    bench_build_index_ja_100k,
    bench_ja_kanji_heavy,
    bench_plain_search,
    bench_plain_search_100k,
    bench_ja_search,
    bench_ja_search_100k,
    bench_zh_search,
    bench_zh_search_100k,
    bench_plain_search_1m
);
criterion_main!(benches);

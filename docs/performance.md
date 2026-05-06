# Performance

These numbers are from the current Criterion suite on a macOS Apple Silicon development machine with Rust 1.94. They are directional, not a cross-machine guarantee.

| Benchmark | Typical time |
| --- | ---: |
| `plain_build_index_100k` | ~13.3 ms |
| `ja_build_index_100k` | ~9.66 ms |
| `plain_search_10k_read` | ~0.92 ms |
| `plain_search_100k_read` | ~4.21 ms |
| `ja_search_10k_kamera` | ~0.35 ms |
| `ja_search_100k_kamera` | ~1.11 ms |
| `zh_search_10k_bjdx` | ~0.34 ms |
| `zh_search_100k_bjdx` | ~1.08 ms |
| `large/plain_search_1m_read_opt_in` | ~36.3 ms |
| `kanji_heavy/worst_ja_build_index_10k` | ~136 ms |
| `kanji_heavy/worst_ja_search_100k_hit` | ~17.2 ms |
| `kanji_heavy/worst_ja_search_100k_nohit` | ~9.46 ms |

Run the same suite locally:

```sh
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
YURU_BENCH_KANJI_HEAVY=1 ./scripts/bench
```

The main hot paths are candidate key construction for language-heavy inputs and fuzzy scoring over large candidate sets. Yuru keeps phonetic work candidate-side and returns `key_index` from search so highlighting is computed only for visible or accepted results.

For the implementation model behind these numbers, see
[architecture and optimization](internals.md). That document covers bounded
candidate keys, query variants, lazy/streaming candidate construction,
background search workers, and preview workers.

# Contributing

Thanks for working on Yuru. Keep changes small, benchmark matching changes, and prefer existing crate boundaries.

## Local Checks

Install hooks:

```sh
./scripts/install-hooks
```

Run the same local quality gate used by CI:

```sh
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
YURU_BENCH_KANJI_HEAVY=1 ./scripts/bench
```

On macOS, `scripts/cargo-env` selects Apple clang and the SDK path. If your shell has another `cc` earlier in `PATH`, source that script or set `CC=/usr/bin/cc`.

## Compatibility

fzf-shaped flags must either work, warn clearly, or fail clearly under `--fzf-compat strict`.

Before v1.0, CLI flags may change, but breaking changes should be documented in `CHANGELOG.md`. Shell integration command names and common scripting flags are treated as stable unless the changelog says otherwise.

## Benchmark Policy

Run benchmarks for any change that touches matching, indexing, language backends, field transforms, walker behavior, or TUI refresh/search loops. Use `YURU_SKIP_BENCH=1` only for a temporary local checkpoint.

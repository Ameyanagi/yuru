# QA harnesses

Opt-in tooling for questions the unit test suite cannot answer: *did the output
change*, *did it get slower*, and *does the interactive interface behave when
keystrokes arrive faster than searches finish*.

None of this runs in CI. It needs a second binary to compare against and takes
minutes, not seconds. Reach for it when changing the matcher, ranking, the
renderer's width accounting, or the TUI event loop.

## Why these exist

During the work that became 0.2.0, the workspace test suite was green at every
step and still missed:

- a change to default-path ranking order for every mixed-case candidate, because
  `tests/golden_ranking.rs` contained no mixed-case data
- `Enter` returning a line the live query did not match, because nothing tested
  keystrokes racing an in-flight search
- a quadratic case-insensitive substring scan, because nothing timed an
  adversarial input

`diffout.py` found the first in one run. The `pty/` drivers found the second.
Test *count* was never the problem; coverage *shape* was.

## Setup

```sh
cargo build --release -p yuru          # binary under test
scripts/qa/build-baseline v0.1.11      # binary to compare against
python3 scripts/qa/gen_corpus.py       # deterministic corpora
```

Everything lands in `target/qa/`, which is already gitignored. Corpora are
seeded, so regenerating reproduces byte-identical files — worth knowing, because
a silently truncated corpus once made a benchmark look 3x better than it was.

Overrides: `YURU_QA_BIN`, `YURU_QA_BASELINE`, `YURU_QA_DIR`.

## Differential output testing

```sh
python3 scripts/qa/diffout.py          # 257 invocations, both binaries, byte compare
python3 scripts/qa/classify.py         # splits differences: pure reorder vs content
```

Runs the same command through both binaries and compares stdout exactly, across
plain and extended queries, every tiebreak and scheme, all four matcher algos,
every case flag combination, field transforms and delimiters, all language
backends, and the output-shape flags.

Any difference is either intended or a regression — there is no third category.
Classify every one. `classify.py` separates "same lines, different order" from
"different lines", which is the distinction that matters when a ranking change is
deliberate: a reorder interacting with `--limit` looks like a content change
until you check the unbounded result.

## Benchmarking

```sh
python3 scripts/qa/bench.py <label>    # absolute timings, min of 5
python3 scripts/qa/ab.py               # interleaved A/B against the baseline
```

**Prefer `ab.py`.** It alternates the two binaries within each case so both see
the same machine conditions. Comparing a fresh `bench.py` run against a recorded
one from an hour ago mixes the code change with whatever else the machine started
doing — during development that produced apparent 30% regressions on code paths
that had not been touched, all of which came out at exactly 1.00x under `ab.py`.

Keep an untouched control case in any comparison. A control that does not read
1.00x means the measurement is wrong, not the code.

Criterion benches (`cargo bench`, or `scripts/bench`) cover the library in
isolation. Ignore criterion's own `change:` percentages unless the stored
baseline is known — measure absolute times on both trees instead.

## Interactive / pty harnesses

```sh
python3 scripts/qa/pty/race_accept.py BIN CORPUS SETTLED EXTRA [SETTLE] [LABEL] [ARGS...]
python3 scripts/qa/pty/drive_tui.py     # general driver
python3 scripts/qa/pty/pty_frame.py     # capture one painted frame
```

`race_accept.py` types `SETTLED`, waits for that search to land, then sends
`EXTRA` plus `Enter` in a single burst so `Enter` is processed before the
requeued search completes. It prints the accepted line. The TUI paints on stderr
and prints the selection on stdout, so the two are separable.

Two scenarios worth keeping green:

```sh
# A: live query matches nothing -> must refuse (exit 1, no output)
python3 scripts/qa/pty/race_accept.py "$BIN" target/qa/big.txt ab '%' 4.0

# B: live set non-empty but different membership -> must never accept a third row
python3 scripts/qa/pty/race_accept.py "$BIN" target/qa/race3.txt ab "$(printf 'C\020')" 5.0 b --live-smart-case
```

A timing bug cannot be evidenced by a unit test. If a fix for one of these is
proposed, reproduce it in a pty before and after.

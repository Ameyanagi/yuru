#!/usr/bin/env python3
"""Benchmark harness: python3 bench.py <label>

Runs each case 5 times against target/release/yuru, feeding a corpus on stdin,
and records the minimum wall-clock time.
"""

import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

import common  # noqa: E402
BINARY = common.BINARY
BIG = common.BIG
CJK = common.CJK
RUNS = 5

CASES = [
    ("floor", BIG, ["--filter", "qqqqqq", "--limit", "10"]),
    ("one-term", BIG, ["--filter", "ab", "--limit", "10"]),
    ("extended-2", BIG, ["--filter", "ab cd", "--limit", "10"]),
    ("extended-2-noext", BIG, ["--filter", "ab cd", "--no-extended", "--limit", "10"]),
    ("extended-4", BIG, ["--filter", "ab cd ef gh", "--limit", "10"]),
    ("topk-1000", BIG, ["--filter", "", "--limit", "1000"]),
    ("nolimit-2000", BIG, ["--filter", "", "--limit", "2000"]),
    ("select-abc", BIG, ["--filter", "abc", "--limit", "1000"]),
    ("nth-plain", BIG, ["--filter", "qqqqqq", "--limit", "10", "--nth", "2"]),
    ("nth-delim", BIG, ["--filter", "qqqqqq", "--limit", "10", "--nth", "2", "-d", "/"]),
    ("ja-index", CJK, ["--lang", "ja", "--filter", "nihongo", "--limit", "100"]),
]


def run_case(corpus: str, argv: list) -> list:
    times = []
    for _ in range(RUNS):
        with open(corpus, "rb") as stdin:
            start = time.perf_counter()
            subprocess.run(
                [BINARY] + argv,
                stdin=stdin,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            times.append(time.perf_counter() - start)
    return times


def main() -> None:
    label = sys.argv[1] if len(sys.argv) > 1 else "baseline"
    best = {}
    allruns = {}
    for name, corpus, argv in CASES:
        times = run_case(corpus, argv)
        best[name] = min(times)
        allruns[name] = times
        print("%s\t%.4f" % (name, best[name]))
        sys.stdout.flush()

    with open(os.path.join(common.ensure_work(), "bench-%s.json" % label), "w") as fh:
        json.dump(best, fh, indent=2)
    with open(os.path.join(common.ensure_work(), "bench-%s-allruns.json" % label), "w") as fh:
        json.dump(allruns, fh, indent=2)


if __name__ == "__main__":
    main()

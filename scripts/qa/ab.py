#!/usr/bin/env python3
"""Interleaved A/B harness: runs the baseline and after binaries alternately on
the SAME cases as bench.py, so both see identical machine conditions."""

import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

import common  # noqa: E402
AFTER = common.BINARY
BASE = common.BASELINE
BIG = common.BIG
CJK = common.CJK
RUNS = int(os.environ.get("AB_RUNS", "7"))

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


def one(binary, corpus, argv):
    with open(corpus, "rb") as stdin:
        start = time.perf_counter()
        subprocess.run(
            [binary] + argv,
            stdin=stdin,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return time.perf_counter() - start


def main():
    out = {}
    print("case\tbase\tafter\tratio")
    for name, corpus, argv in CASES:
        bt, at = [], []
        for _ in range(RUNS):
            bt.append(one(BASE, corpus, argv))
            at.append(one(AFTER, corpus, argv))
        b, a = min(bt), min(at)
        out[name] = {"base": b, "after": a, "ratio": a / b,
                     "base_runs": bt, "after_runs": at}
        print("%s\t%.4f\t%.4f\t%.2fx" % (name, b, a, a / b))
        sys.stdout.flush()
    with open(os.path.join(common.ensure_work(), "ab-results.json"), "w") as fh:
        json.dump(out, fh, indent=2)


if __name__ == "__main__":
    main()

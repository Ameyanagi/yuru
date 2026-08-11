#!/usr/bin/env python3
"""Shared path resolution for the QA harnesses.

Everything is repo-relative by default and overridable by environment variable,
so the harnesses run from a clean checkout with no editing.

  YURU_QA_BIN       binary under test        (default <repo>/target/release/yuru)
  YURU_QA_BASELINE  binary to compare with   (default <repo>/target/qa/baseline/yuru)
  YURU_QA_DIR       work directory for generated corpora and results
                    (default <repo>/target/qa, which is already gitignored)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

WORK = os.environ.get("YURU_QA_DIR") or os.path.join(REPO, "target", "qa")
BINARY = os.environ.get("YURU_QA_BIN") or os.path.join(REPO, "target", "release", "yuru")
BASELINE = os.environ.get("YURU_QA_BASELINE") or os.path.join(WORK, "baseline", "yuru")

BIG = os.path.join(WORK, "big.txt")
CJK = os.path.join(WORK, "cjk.txt")
CASE = os.path.join(WORK, "case.txt")
TINY = os.path.join(WORK, "tiny.txt")
RACE = os.path.join(WORK, "race3.txt")


def ensure_work() -> str:
    os.makedirs(WORK, exist_ok=True)
    return WORK


def require(path: str, what: str, hint: str) -> str:
    if not os.path.exists(path):
        sys.exit(f"missing {what}: {path}\n  {hint}")
    return path


def require_binary() -> str:
    return require(
        BINARY,
        "binary under test",
        "build it with `cargo build --release -p yuru`, or set YURU_QA_BIN",
    )


def require_baseline() -> str:
    return require(
        BASELINE,
        "baseline binary",
        "build one with `scripts/qa/build-baseline`, or set YURU_QA_BASELINE",
    )


def require_corpora(*paths: str) -> None:
    for path in paths:
        require(
            path,
            "corpus",
            "generate corpora with `python3 scripts/qa/gen_corpus.py`",
        )

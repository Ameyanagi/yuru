#!/usr/bin/env python3
"""Differential output test: baseline binary vs current binary.

Runs the same (args, corpus) combination through both and compares stdout byte
for byte. Any difference is reported with a small excerpt so it can be
classified as intended or not.
"""
import itertools
import subprocess
import sys

import common

S = common.WORK
BASE = f"{S}/base/target/release/yuru"
NEW = common.BINARY

BIG = f"{S}/big.txt"
CJK = f"{S}/cjk.txt"
CASE = f"{S}/case.txt"


def run(binary, args, corpus):
    with open(corpus, "rb") as fh:
        p = subprocess.run(
            [binary] + args,
            stdin=fh,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={"PATH": "/usr/bin:/bin", "HOME": "/tmp", "YURU_FZF_COMPAT": "ignore"},
        )
    return p.returncode, p.stdout


def cases():
    # (label, args, corpus)
    # --- plain single-term fuzzy, various limits/tiebreaks/algos ---
    for q in ["read", "src", "abc", "foo", "httpserver", "testcase", "zzz", "a"]:
        for extra in ([], ["--limit", "50"], ["--limit", "1000"], ["--limit", "1"]):
            yield (f"plain q={q} {extra}", ["--filter", q] + extra, CASE)
    for q in ["ab", "abc", "file123", "qqqq"]:
        yield (f"big q={q}", ["--filter", q, "--limit", "200"], BIG)

    # --- tiebreaks (ranking-sensitive) ---
    for tb in ["length", "begin", "end", "chunk", "pathname", "index",
               "begin,length", "chunk,begin", "pathname,length"]:
        yield (f"tiebreak={tb}", ["--filter", "read", "--tiebreak", tb, "--limit", "100"], CASE)
    for scheme in ["default", "path", "history"]:
        yield (f"scheme={scheme}", ["--filter", "src", "--scheme", scheme, "--limit", "100"], CASE)

    # --- matcher algos ---
    for algo in ["greedy", "fzf-v1", "fzf-v2", "nucleo"]:
        yield (f"algo={algo}", ["--filter", "readme", "--algo", algo, "--limit", "100"], CASE)
        yield (f"algo={algo} multi", ["--filter", "src read", "--algo", algo, "--limit", "100"], CASE)

    # --- case flags: THE change under suspicion ---
    for q in ["read", "READ", "ReadMe", "abc", "ABC", "AbcDef", "http", "HTTP"]:
        for flags in ([], ["--ignore-case"], ["--no-ignore-case"],
                      ["--literal"], ["--ignore-case", "--literal"],
                      ["--no-ignore-case", "--literal"]):
            yield (f"case q={q} {flags}", ["--filter", q, "--limit", "100"] + flags, CASE)

    # --- extended syntax: the rewritten path ---
    ext = ["src read", "read md", "'readme", "^Src", "TXT$", "!md", "read !md",
           "src | docs", "'read | 'src", "^Src read TXT$", "abc def ghi",
           "read !md !txt", "'ReadMe$", "^src 'main", "a b c d"]
    for q in ext:
        for extra in ([], ["--exact"], ["--extended-exact"], ["--limit", "25"]):
            yield (f"ext q={q!r} {extra}", ["--filter", q, "--limit", "100"] + extra, CASE)
    for q in ["ab cd", "ab cd ef gh", "^a file$", "ab !cd"]:
        yield (f"ext-big q={q!r}", ["--filter", q, "--limit", "200"], BIG)

    # --- exact / disabled / no-sort ---
    for flags in (["--exact"], ["--no-extended"], ["--no-sort"],
                  ["--disabled"], ["--exact", "--no-sort"]):
        yield (f"mode {flags}", ["--filter", "read", "--limit", "100"] + flags, CASE)

    # --- field transforms + delimiter (Phase 3b) ---
    for d, nth in [(None, "1"), (None, "2"), (None, "-1"), (None, "2.."),
                   ("/", "1"), ("/", "2"), ("/", "-1"), ("/", ".."),
                   (r"\.", "1"), ("[/_]", "2"), ("-", "1")]:
        args = ["--filter", "read", "--limit", "100", "--nth", nth]
        if d:
            args += ["-d", d]
        yield (f"nth={nth} d={d}", args, CASE)
        args2 = ["--filter", "read", "--limit", "50", "--with-nth", nth]
        if d:
            args2 += ["-d", d]
        yield (f"with-nth={nth} d={d}", args2, CASE)
        args3 = ["--filter", "read", "--limit", "50", "--accept-nth", nth]
        if d:
            args3 += ["-d", d]
        yield (f"accept-nth={nth} d={d}", args3, CASE)

    # --- CJK / language backends ---
    for lang in ["plain", "ja", "zh", "ko", "all"]:
        for q in ["nihongo", "tokyo", "kamera", "shashin", "bjdx", "日本語", "カメラ", "検索"]:
            yield (f"lang={lang} q={q}", ["--lang", lang, "--filter", q, "--limit", "60"], CJK)

    # --- explain / debug output (known intentional change) ---
    for q in ["read", "abc"]:
        yield (f"explain q={q}", ["--explain", "--filter", q, "--limit", "10"], CASE)
    for q in ["read", "nihongo"]:
        yield (f"variants q={q}", ["--debug-query-variants", "--filter", q], CASE)

    # --- misc output-shape flags ---
    for flags in (["--print-query"], ["--print0"], ["--tac"], ["--tail", "500"],
                  ["--header-lines", "3"], ["--select-1"], ["--exit-0"]):
        yield (f"shape {flags}", ["--filter", "read", "--limit", "20"] + flags, CASE)


def main():
    total = same = 0
    diffs = []
    for label, args, corpus in cases():
        total += 1
        rc_b, out_b = run(BASE, args, corpus)
        rc_n, out_n = run(NEW, args, corpus)
        if rc_b == rc_n and out_b == out_n:
            same += 1
            continue
        lb = out_b.decode("utf-8", "replace").splitlines()
        ln = out_n.decode("utf-8", "replace").splitlines()
        first = next((i for i, (x, y) in enumerate(itertools.zip_longest(lb, ln)) if x != y), 0)
        diffs.append({
            "label": label, "args": args, "corpus": corpus.rsplit("/", 1)[-1],
            "rc": (rc_b, rc_n), "nlines": (len(lb), len(ln)),
            "first_diff_line": first,
            "base": lb[first:first + 3], "new": ln[first:first + 3],
        })

    print(f"cases={total}  identical={same}  differing={len(diffs)}")
    print()
    for d in diffs:
        print("=" * 72)
        print(f"CASE   : {d['label']}")
        print(f"ARGS   : {' '.join(d['args'])}   [{d['corpus']}]")
        print(f"EXIT   : base={d['rc'][0]} new={d['rc'][1]}")
        print(f"LINES  : base={d['nlines'][0]} new={d['nlines'][1]}   first diff at line {d['first_diff_line']}")
        print(f"  BASE : {d['base']}")
        print(f"  NEW  : {d['new']}")
    return 0 if not diffs else 1


if __name__ == "__main__":
    sys.exit(main())

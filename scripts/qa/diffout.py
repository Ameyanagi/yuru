#!/usr/bin/env python3
"""Differential output test: baseline binary vs current binary.

Two passes, because they answer different questions.

*Ordered pass* (`cases()`, 257 invocations): runs the same (args, corpus)
combination through both binaries and compares stdout byte for byte. Every case
pins `--limit`, so this is a comparison of an ordered top-N. It is what catches
ranking changes, and its counts are quoted in CHANGELOG.md - do not renumber it.

*Set pass* (`set_cases()`): reruns the families where the matching set could
plausibly change, with no `--limit` at all, and compares the multiset of output
records rather than their order. The ordered pass is blind to any change that
stays below the limit: `--filter ABC --ignore-case --literal` went from 1355 to
3973 matched lines between 0.1.11 and 0.2.0 and the ordered pass called the case
identical, because the top 100 was unchanged. Reordering is expected noise here
and is deliberately not reported; only membership and exit code are.
"""
import itertools
import subprocess
import sys
from collections import Counter

import common

BASE = common.BASELINE
NEW = common.BINARY

BIG = common.BIG
CJK = common.CJK
CASE = common.CASE


# Shared between the two passes so they cannot drift apart.
CASE_QUERIES = ["read", "READ", "ReadMe", "abc", "ABC", "AbcDef", "http", "HTTP"]
CASE_FLAGS = ([], ["--ignore-case"], ["--no-ignore-case"],
              ["--literal"], ["--ignore-case", "--literal"],
              ["--no-ignore-case", "--literal"])
EXT_QUERIES = ["src read", "read md", "'readme", "^Src", "TXT$", "!md", "read !md",
               "src | docs", "'read | 'src", "^Src read TXT$", "abc def ghi",
               "read !md !txt", "'ReadMe$", "^src 'main", "a b c d"]
PLAIN_QUERIES = ["read", "src", "abc", "foo", "httpserver", "testcase", "zzz", "a"]
ALGOS = ["greedy", "fzf-v1", "fzf-v2", "nucleo"]
LANGS = ["plain", "ja", "zh", "ko", "all"]
CJK_QUERIES = ["nihongo", "tokyo", "kamera", "shashin", "bjdx", "日本語", "カメラ", "検索"]


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
    for q in PLAIN_QUERIES:
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
    # NOTE: every query here is lowercase, so this family says nothing about how
    # fzf-v2/nucleo handle an uppercase query. set_cases() covers that.
    for algo in ALGOS:
        yield (f"algo={algo}", ["--filter", "readme", "--algo", algo, "--limit", "100"], CASE)
        yield (f"algo={algo} multi", ["--filter", "src read", "--algo", algo, "--limit", "100"], CASE)

    # --- case flags: THE change under suspicion ---
    for q in CASE_QUERIES:
        for flags in CASE_FLAGS:
            yield (f"case q={q} {flags}", ["--filter", q, "--limit", "100"] + flags, CASE)

    # --- extended syntax: the rewritten path ---
    for q in EXT_QUERIES:
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
    for lang in LANGS:
        for q in CJK_QUERIES:
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


def set_cases():
    """Unbounded reruns of the families whose *matching set* could move.

    No `--limit` anywhere: the point is to see candidates the ordered pass
    truncates away. Same (label, args, corpus) shape as cases(); these are
    counted and reported separately and never renumber the 257.
    """
    # --- case flags: where 0.2.0 actually changed which lines match ---
    for q in CASE_QUERIES:
        for flags in CASE_FLAGS:
            yield (f"set case q={q} {flags}", ["--filter", q] + flags, CASE)

    # --- case flags x algo: the ordered pass only ever runs lowercase queries
    # through fzf-v2/nucleo, so it cannot see either the case-policy fix or the
    # uppercase-query abort on those backends. Exit codes are compared too.
    for algo in ALGOS:
        for q in CASE_QUERIES:
            for flags in ([], ["--ignore-case"], ["--no-ignore-case"]):
                yield (f"set algo={algo} q={q} {flags}",
                       ["--filter", q, "--algo", algo] + flags, CASE)

    # --- plain and extended, unbounded ---
    for q in PLAIN_QUERIES:
        yield (f"set plain q={q}", ["--filter", q], CASE)
    for q in EXT_QUERIES:
        for extra in ([], ["--exact"], ["--extended-exact"]):
            yield (f"set ext q={q!r} {extra}", ["--filter", q] + extra, CASE)

    # --- language backends, unbounded ---
    for lang in LANGS:
        for q in CJK_QUERIES:
            yield (f"set lang={lang} q={q}", ["--lang", lang, "--filter", q], CJK)


def records(args, out):
    """Split stdout into comparable records, honouring the output separator."""
    sep = b"\0" if "--print0" in args else b"\n"
    return Counter(r for r in out.split(sep) if r)


def excerpt(counter, limit=3):
    return [r.decode("utf-8", "replace") for r, _ in itertools.islice(counter.items(), limit)]


def ordered_pass():
    """Byte compare of the 257 bounded cases. Returns the number differing."""
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
    print("(bounded by --limit; ordered byte compare - a change below the limit is invisible)")
    print()
    for d in diffs:
        print("=" * 72)
        print(f"CASE   : {d['label']}")
        print(f"ARGS   : {' '.join(d['args'])}   [{d['corpus']}]")
        print(f"EXIT   : base={d['rc'][0]} new={d['rc'][1]}")
        print(f"LINES  : base={d['nlines'][0]} new={d['nlines'][1]}   first diff at line {d['first_diff_line']}")
        print(f"  BASE : {d['base']}")
        print(f"  NEW  : {d['new']}")
    return len(diffs)


def set_pass():
    """Unbounded membership compare. Returns the number whose set changed."""
    total = same = 0
    diffs = []
    for label, args, corpus in set_cases():
        total += 1
        rc_b, out_b = run(BASE, args, corpus)
        rc_n, out_n = run(NEW, args, corpus)
        rb, rn = records(args, out_b), records(args, out_n)
        if rc_b == rc_n and rb == rn:
            same += 1
            continue
        diffs.append({
            "label": label, "args": args, "corpus": corpus.rsplit("/", 1)[-1],
            "rc": (rc_b, rc_n), "n": (sum(rb.values()), sum(rn.values())),
            "only_base": rb - rn, "only_new": rn - rb,
        })

    print("#" * 72)
    print(f"set-cases={total}  same-set={same}  set-changed={len(diffs)}")
    print("(unbounded; membership and exit code only - reordering is not reported)")
    print()
    for d in diffs:
        print("=" * 72)
        print(f"CASE     : {d['label']}")
        print(f"ARGS     : {' '.join(d['args'])}   [{d['corpus']}]")
        print(f"EXIT     : base={d['rc'][0]} new={d['rc'][1]}")
        print(f"MATCHED  : base={d['n'][0]} new={d['n'][1]}"
              f"   only-in-base={sum(d['only_base'].values())}"
              f" only-in-new={sum(d['only_new'].values())}")
        if d["only_base"]:
            print(f"  BASE   : {excerpt(d['only_base'])}")
        if d["only_new"]:
            print(f"  NEW    : {excerpt(d['only_new'])}")
    return len(diffs)


def main():
    common.require_binary()
    common.require_baseline()
    common.require_corpora(BIG, CJK, CASE)
    print(f"baseline: {BASE}")
    print(f"under test: {NEW}")
    print()

    differing = ordered_pass()
    print()
    changed = set_pass()
    return 0 if not differing and not changed else 1


if __name__ == "__main__":
    sys.exit(main())

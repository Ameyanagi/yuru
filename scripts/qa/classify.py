#!/usr/bin/env python3
"""Classify each differing case: pure reorder, or genuine content change?"""
import collections
import sys

import common

sys.path.insert(0, common.WORK)
from diffout import cases, run, BASE, NEW  # noqa: E402

buckets = collections.Counter()
content_changed = []
reorder_only = []

for label, args, corpus in cases():
    rc_b, out_b = run(BASE, args, corpus)
    rc_n, out_n = run(NEW, args, corpus)
    if rc_b == rc_n and out_b == out_n:
        continue
    lb = out_b.decode("utf-8", "replace").splitlines()
    ln = out_n.decode("utf-8", "replace").splitlines()
    if sorted(lb) == sorted(ln):
        buckets["reorder_only"] += 1
        reorder_only.append(label)
    else:
        buckets["content_changed"] += 1
        only_b = sorted(set(lb) - set(ln))
        only_n = sorted(set(ln) - set(lb))
        content_changed.append({
            "label": label, "args": args, "corpus": corpus.rsplit("/", 1)[-1],
            "only_in_base": only_b[:6], "only_in_new": only_n[:6],
            "n_only_base": len(only_b), "n_only_new": len(only_n),
        })

print("SUMMARY:", dict(buckets))
print()
print(f"--- PURE REORDER ({len(reorder_only)} cases): same lines, different order ---")
for lbl in reorder_only:
    print("   ", lbl)
print()
print(f"--- CONTENT CHANGED ({len(content_changed)} cases): different lines ---")
for c in content_changed:
    print("=" * 72)
    print(f"CASE : {c['label']}")
    print(f"ARGS : {' '.join(c['args'])}   [{c['corpus']}]")
    print(f"  only in BASE ({c['n_only_base']}): {c['only_in_base']}")
    print(f"  only in NEW  ({c['n_only_new']}): {c['only_in_new']}")

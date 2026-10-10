#!/usr/bin/env python3
"""Diff two live sets written by `alloc-trace-report.py --dump-live`.

Usage: alloc-live-diff.py A.tsv B.tsv [--top N]

Prints B − A by subsystem, then the owners that moved most, largest first.
"""

import argparse
from collections import defaultdict


def load(path):
    rows = {}
    with open(path) as f:
        next(f)
        for line in f:
            byt, cnt, sub, owner = line.rstrip("\n").split("\t", 3)
            rows[(sub, owner)] = (int(byt), int(cnt))
    return rows


ap = argparse.ArgumentParser()
ap.add_argument("a")
ap.add_argument("b")
ap.add_argument("--top", type=int, default=25)
args = ap.parse_args()
a, b = load(args.a), load(args.b)
ta = sum(v[0] for v in a.values())
tb = sum(v[0] for v in b.values())
print(f"A {args.a}: {ta:,} B; B {args.b}: {tb:,} B; B − A = {tb - ta:+,} B")
print()
subs = defaultdict(lambda: [0, 0])
for (s, _), (byt, _) in a.items():
    subs[s][0] += byt
for (s, _), (byt, _) in b.items():
    subs[s][1] += byt
print("| subsystem | A | B | B − A |")
print("|---|---:|---:|---:|")
for s, (x, y) in sorted(subs.items(), key=lambda kv: -abs(kv[1][1] - kv[1][0])):
    if x != y:
        print(f"| {s} | {x:,} | {y:,} | {y - x:+,} |")
print()
keys = set(a) | set(b)
moved = sorted(keys, key=lambda k: -abs(b.get(k, (0, 0))[0] - a.get(k, (0, 0))[0]))
print("| B − A | blocks A→B | subsystem | owner |")
print("|---:|---|---|---|")
for k in moved[: args.top]:
    x, y = a.get(k, (0, 0)), b.get(k, (0, 0))
    if x[0] == y[0]:
        break
    print(f"| {y[0] - x[0]:+,} | {x[1]}→{y[1]} | {k[0]} | `{k[1]}` |")

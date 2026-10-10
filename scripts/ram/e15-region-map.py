#!/usr/bin/env python3
"""E15: what holds one heap region open, at every compile start.

Reads an allocation trace (`lp-cli emu run --alloc-trace`) and, at the start
of every shader compile (the `[mem] shader compile before` log record), lists
the region's live blocks in address order — offset, size, owner, and the
window it was born in — and its largest hole. Then a summary: the region's
largest hole per compile start, and the owners whose blocks bound it.

A block's *birth window* is named by the compile markers around it:
`c<n>` (inside compile n: `compilation starting` .. `compilation succeeded`),
`pre<n>` (after compile n-1 finished, before compile n started).

Usage:
    e15-region-map.py TRACE --elf p2.elf [--region dram2] [--detail]
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import statistics
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("atr", os.path.join(HERE, "alloc-trace-report.py"))
atr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(atr)

START = "compilation starting"
DONE = "compilation succeeded"
BEFORE = "[mem] shader compile before"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--region", default="dram2")
    ap.add_argument("--detail", action="store_true", help="list every live block per point")
    ap.add_argument("--depth", type=int, default=2)
    ap.add_argument("--end", action="store_true", help="add a point at the end of the trace")
    ap.add_argument("--runs", action="store_true",
                    help="per point, the region as runs of live blocks (gaps < 64 B merged): "
                         "offset, span, bytes, birth windows, the run's biggest owner")
    args = ap.parse_args()

    regions = {n: (s, s + z) for n, s, z in atr.heap_regions(args.elf)}
    if args.region not in regions:
        print(f"no region {args.region}; have {', '.join(regions)}", file=sys.stderr)
        return 1
    lo, hi = regions[args.region]
    sym = atr.Symbolizer(args.elf)

    recs: list[tuple[int, str, str]] = []  # size, frames, birth window
    live: dict[int, int] = {}
    compiles = 0
    in_compile = False
    boot = 0
    points = []  # (n, {ptr: rid}) for this region at each compile start

    with open(args.trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                _, _cyc, ptr, size, _caps, frames = line.split(" ", 5)
                ptr = int(ptr, 16)
                if ptr == 0:
                    continue
                window = f"c{compiles}" if in_compile else f"pre{compiles + 1}"
                recs.append((int(size), frames.strip(), window))
                live[ptr] = len(recs) - 1
            elif tag == "F":
                live.pop(int(line.split(" ", 3)[2], 16), None)
            elif tag == "L":
                text = line.rstrip("\n").split(" ", 2)[2]
                if text.startswith("@reboot"):
                    boot += 1
                    live = {}
                elif START in text:
                    compiles += 1
                    in_compile = True
                elif DONE in text:
                    in_compile = False
                elif BEFORE in text:
                    points.append((compiles, {p: r for p, r in live.items() if lo <= p < hi}))
    if args.end:
        points.append(("end", {p: r for p, r in live.items() if lo <= p < hi}))

    def owner(rid: int) -> str:
        names = [sym(int(x, 16)) for x in recs[rid][1].split(",") if x]
        for i, n in enumerate(names):
            if not atr.is_machinery(n):
                return " < ".join(atr.HASH_SUFFIX.sub("", m) for m in names[i : i + args.depth])
        return "?"

    largest_by_point = []
    bounders = defaultdict(lambda: [0, 0])
    print(f"# e15-region-map: {args.trace} region {args.region} "
          f"0x{lo:08x}..0x{hi:08x} ({hi - lo:,} B); {boot} reboot(s)")
    print()
    print("| compile | live B | blocks | largest hole | hole at | bounded below by | bounded above by |")
    print("|---:|---:|---:|---:|---|---|---|")
    for n, blocks in points:
        items = sorted((p, recs[r][0], r) for p, r in blocks.items())
        cursor, best = lo, (0, lo, None, None)
        prev = None
        for p, sz, r in items:
            gap = p - cursor
            if gap > best[0]:
                best = (gap, cursor, prev, r)
            cursor = max(cursor, p + sz)
            prev = r
        if hi - cursor > best[0]:
            best = (hi - cursor, cursor, prev, None)
        gap, at, below, above = best
        if n != "end":
            largest_by_point.append(gap)
        def name(r):
            if r is None:
                return "(region edge)"
            return f"{recs[r][0]} B {recs[r][2]} {owner(r)}"
        for r in (below, above):
            if r is not None:
                bounders[owner(r)][0] += 1
                bounders[owner(r)][1] = max(bounders[owner(r)][1], recs[r][0])
        print(f"| {n} | {sum(s for _, s, _ in items):,} | {len(items)} | {gap:,} | +{at - lo} "
              f"| {name(below)[:110]} | {name(above)[:110]} |")
        if args.runs:
            print_runs(items, lo, recs, owner)
        if args.detail:
            for p, sz, r in items:
                print(f"|  | +{p - lo} | {sz} | {recs[r][2]} | {owner(r)[:140]} | | |")
    print()
    if largest_by_point:
        print(f"largest hole at compile starts: min {min(largest_by_point):,} / median "
              f"{int(statistics.median(largest_by_point)):,} / max {max(largest_by_point):,} B "
              f"over {len(largest_by_point)} compile starts")
    print()
    print("| owner bounding the largest hole | times | largest block |")
    print("|---|---:|---:|")
    for o, (cnt, big) in sorted(bounders.items(), key=lambda kv: -kv[1][0]):
        print(f"| {o[:150]} | {cnt} | {big} |")
    return 0


def print_runs(items, lo, recs, owner) -> None:
    """The region's live blocks as runs (a gap under 64 B does not split one)."""
    runs: list[list[tuple[int, int, int]]] = []
    last_end = None
    for p, sz, r in items:
        if last_end is None or p - last_end >= 64:
            runs.append([])
        runs[-1].append((p, sz, r))
        last_end = max(last_end or 0, p + sz)
    for run in runs:
        start = run[0][0]
        end = max(p + sz for p, sz, _ in run)
        by_owner = defaultdict(int)
        for _, sz, r in run:
            by_owner[owner(r).split(" < ")[0]] += sz
        top = max(by_owner.items(), key=lambda kv: kv[1])
        wins = sorted({recs[r][2] for _, _, r in run})
        print(f"|  | +{start - lo}..+{end - lo} | {sum(sz for _, sz, _ in run):,} B in {len(run)} "
              f"| {','.join(wins)} | {top[0][:90]} ({top[1]} B) | | |")


if __name__ == "__main__":
    sys.exit(main())

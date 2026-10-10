#!/usr/bin/env python3
"""List the live heap blocks born between two points of an allocation trace,
one row per distinct (size, call chain), with the region each landed in.

The companion of `alloc-trace-report.py` for the radio experiment (E13): that
script groups by owner; this one keeps the block size, so a controller's
`r_ble_ll_mem_alloc` calls can be read as "n blocks of s bytes from site X"
and matched against a config field (an ACL buffer count, an event-buffer
count, a list length).

Usage:
    radio-alloc-sites.py TRACE --elf p2.elf --from 'TEXT' --to 'TEXT' [--min 16] [--frames 6]

`--from`/`--to` are substrings of a guest log record (an `L` line), the first
match after power-on. Standard library only; needs `rust-nm` on PATH.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("atr", os.path.join(HERE, "alloc-trace-report.py"))
atr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(atr)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--from", dest="start", required=True)
    ap.add_argument("--to", dest="end", required=True)
    ap.add_argument("--min", type=int, default=0, help="hide rows with total below this")
    ap.add_argument("--frames", type=int, default=5)
    ap.add_argument("--skip-machinery", action="store_true")
    args = ap.parse_args()

    sym = atr.Symbolizer(args.elf)
    regions = atr.heap_regions(args.elf)

    def region_of(ptr: int) -> str:
        for name, start, size in regions:
            if start <= ptr < start + size:
                return name
        return "?"

    live: dict[int, tuple[int, str]] = {}
    born: set[int] | None = None
    in_window = False
    window: dict[int, tuple[int, str]] = {}
    done = False
    with open(args.trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                _, cyc, ptr, size, caps, frames = line.split(" ", 5)
                ptr = int(ptr, 16)
                if ptr == 0:
                    continue
                live[ptr] = (int(size), frames.strip())
                if in_window:
                    window[ptr] = live[ptr]
            elif tag == "F":
                _, cyc, ptr, size = line.split(" ", 3)
                ptr = int(ptr, 16)
                live.pop(ptr, None)
                window.pop(ptr, None)
            elif tag == "L":
                text = line.rstrip("\n").split(" ", 2)[2]
                if not in_window and args.start in text:
                    in_window = True
                elif in_window and args.end in text:
                    done = True
                    break
    if not done:
        print("window markers not found", file=sys.stderr)
        return 1

    groups: dict[tuple, list[int]] = defaultdict(list)
    for ptr, (size, frames) in window.items():
        rets = [int(x, 16) for x in frames.split(",") if x]
        names = [sym(r) for r in rets]
        if args.skip_machinery:
            names = [n for n in names if not atr.is_machinery(n)]
        chain = tuple(n[:70] for n in names[: args.frames])
        groups[(size, region_of(ptr), chain)].append(size)

    rows = sorted(groups.items(), key=lambda kv: -(kv[0][0] * len(kv[1])))
    total = 0
    print(f"{'total':>7} {'n':>4} {'each':>6} {'region':<7} chain")
    for (size, region, chain), sizes in rows:
        t = size * len(sizes)
        total += t
        if t < args.min:
            continue
        print(f"{t:>7} {len(sizes):>4} {size:>6} {region:<7} {' < '.join(chain)}")
    print(f"total requested in window: {total} B in {len(window)} blocks")
    return 0


if __name__ == "__main__":
    sys.exit(main())

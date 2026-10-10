#!/usr/bin/env python3
"""Compare the heap blocks two allocation traces make between the same two
log records, by (size, region, innermost two frames).

For RAM experiment E13: build the firmware twice at two BLE controller
configurations (`ble_cfg_probe`), trace both on the emulator, and read which
blocks a config field added or removed.

Usage:
    radio-alloc-diff.py TRACE_A ELF_A TRACE_B ELF_B [--from TEXT] [--to TEXT]

Defaults window the BLE controller bring-up: `[ble] enabled` .. `[ble] up:`.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("atr", os.path.join(HERE, "alloc-trace-report.py"))
atr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(atr)


def window_blocks(trace: str, elf: str, start: str, end: str, depth: int) -> Counter:
    sym = atr.Symbolizer(elf)
    regions = atr.heap_regions(elf)

    def region_of(ptr: int) -> str:
        for name, base, size in regions:
            if base <= ptr < base + size:
                return name
        return "?"

    window: dict[int, tuple[int, str]] = {}
    in_window = False
    done = False
    with open(trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                _, _cyc, ptr, size, _caps, frames = line.split(" ", 5)
                ptr = int(ptr, 16)
                if ptr and in_window:
                    window[ptr] = (int(size), frames.strip())
            elif tag == "F":
                _, _cyc, ptr, _size = line.split(" ", 3)
                window.pop(int(ptr, 16), None)
            elif tag == "L":
                text = line.rstrip("\n").split(" ", 2)[2]
                if not in_window and start in text:
                    in_window = True
                elif in_window and end in text:
                    done = True
                    break
    if not done:
        raise SystemExit(f"{trace}: window markers not found")
    out: Counter = Counter()
    for ptr, (size, frames) in window.items():
        rets = [int(x, 16) for x in frames.split(",") if x]
        names = [n for n in (sym(r) for r in rets) if not atr.is_machinery(n)]
        chain = " < ".join(n[:40] for n in names[:depth])
        out[(size, region_of(ptr), chain)] += 1
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace_a")
    ap.add_argument("elf_a")
    ap.add_argument("trace_b")
    ap.add_argument("elf_b")
    ap.add_argument("--from", dest="start", default="[ble] enabled")
    ap.add_argument("--to", dest="end", default="[ble] up:")
    ap.add_argument("--depth", type=int, default=2)
    args = ap.parse_args()

    a = window_blocks(args.trace_a, args.elf_a, args.start, args.end, args.depth)
    b = window_blocks(args.trace_b, args.elf_b, args.start, args.end, args.depth)
    total_a = sum(k[0] * n for k, n in a.items())
    total_b = sum(k[0] * n for k, n in b.items())
    net = 0
    rows = sorted(set(a) | set(b), key=lambda k: -abs((b[k] - a[k]) * k[0]))
    for k in rows:
        if a[k] != b[k]:
            delta = (b[k] - a[k]) * k[0]
            net += delta
            print(f"{k[0]:>6} B {k[1]:<6} {k[2]:<82} {a[k]:>3} -> {b[k]:>3} = {delta:+d}")
    print(f"window total A {total_a} B, B {total_b} B, net {net:+d} B (requested bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

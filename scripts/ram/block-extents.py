#!/usr/bin/env python3
"""Name each `S` line of an allocation trace (`lp-cli emu run --alloc-trace`):
the touched extent of every block of 1 KiB or more still live at the run's
end, joined to the allocation that made it.

For a stack block (grows down from the top), `used = size - first` is its
high-water in bytes when the memory under it was never used before (the
emulated machine's RAM starts zeroed), and an upper bound otherwise: a block
whose `first` is 0 sat on reused memory and says nothing about the stack.

Usage:
    block-extents.py TRACE --elf p2.elf [--match esp_rtos_task_create]
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("atr", os.path.join(HERE, "alloc-trace-report.py"))
atr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(atr)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--match", default="", help="only owners containing this text")
    args = ap.parse_args()

    sym = atr.Symbolizer(args.elf)
    last: dict[int, tuple[int, str]] = {}
    extents = []
    with open(args.trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                _, _cyc, ptr, size, _caps, frames = line.split(" ", 5)
                if int(ptr, 16):
                    last[int(ptr, 16)] = (int(size), frames.strip())
            elif tag == "@" or (tag == "M" and "@reboot" in line):
                last.clear()
            elif tag == "S":
                _, ptr, size, first, last_nz, words = line.split()
                extents.append((int(ptr, 16), int(size), int(first), int(last_nz), int(words)))
    print(f"{'ptr':>9} {'size':>6} {'first':>6} {'used(top)':>9} {'words':>5}  owner")
    for ptr, size, first, last_nz, words in extents:
        size_a, frames = last.get(ptr, (size, ""))
        rets = [int(x, 16) for x in frames.split(",") if x]
        names = [n for n in (sym(r) for r in rets) if not atr.is_machinery(n)]
        owner = " < ".join(n[:60] for n in names[:3])
        if args.match and args.match not in owner:
            continue
        used = "-" if first < 0 else str(size - first)
        print(f"{ptr:>9x} {size:>6} {first:>6} {used:>9} {words:>5}  {owner}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

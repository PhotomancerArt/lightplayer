#!/usr/bin/env python3
"""SPIKE (fps spike): turn a console's `[pcs] r.i <hex>...` lines into
symbolize.py's `<core> <pc> <n>` input. Usage: pcs_fold.py <console> > x.pcs"""
import re, sys, collections
c = collections.Counter()
for line in open(sys.argv[1], errors="replace"):
    m = re.search(r"\[pcs\] \d+\.\d+ ((?:[0-9a-f]{8} ?)+)", line)
    if m:
        for h in m.group(1).split():
            c[int(h, 16)] += 1
for pc, n in sorted(c.items()):
    print(f"0 {pc:#x} {n}")

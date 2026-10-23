#!/usr/bin/env python3
"""SPIKE: steady-state fps/tick from emu or board consoles.
Usage: table.py <dir> <glob-suffix e.g. .txt>"""
import re, sys, glob, os, collections
d = sys.argv[1]
rows = collections.defaultdict(dict)
for f in sorted(glob.glob(os.path.join(d, "*--*.txt"))):
    if f.endswith(".run.txt"): continue
    v, p = os.path.basename(f)[:-4].split("--")
    txt = open(f, errors="replace").read()
    perf = re.findall(r"\[perf\] frame=(\d+) fps=(\d+) elapsed=(\d+)ms .*?tick=(\d+)ms", txt)
    p99 = re.findall(r"\[perf\] frames p50≤(\d+)ms p99≤(\d+)ms max=(\d+)ms", txt)
    if len(perf) < 2:
        rows[p][v] = "—"; continue
    # fps from the last two windows' frame counters (exact, not the integer field)
    (f1, _, _, _), (f2, _, el, tick) = perf[-2], perf[-1]
    fps = (int(f2) - int(f1)) * 1000 / int(el)
    pp = p99[-1][1] if p99 else "?"
    rows[p][v] = f"{fps:.1f} fps / {tick} ms (p99≤{pp})"
vs = sorted({v for r in rows.values() for v in r})
print("| project | " + " | ".join(vs) + " |")
print("|---|" + "---:|" * len(vs))
for p in sorted(rows, key=lambda x: (len(x), x)):
    print(f"| {p} | " + " | ".join(rows[p].get(v, "") for v in vs) + " |")

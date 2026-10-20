#!/usr/bin/env python3
"""Reproduce the tables in docs/reports/2026-10-16-c6-time-to-first-light.md.

    python3 summarize.py <records dir>

Reads `first-light-<image>.json` (silicon, `lp-cli/tests/board_first_light.rs`)
and `emu-first-light-<grade>.json` (emulated, `emu_first_light.rs`) and prints
median (min–max) per part, per image, per case, plus the shader compile's
own `elapsed=` from the `[shader-node]` line.
"""

import json
import re
import statistics
import sys
from pathlib import Path

PARTS = ["boot", "init", "fs", "radio", "split", "server", "load", "frame1", "lit"]
LINE = re.compile(r"first light (at|none by) (\d+) ms: (.*?) \(lp-sys (-?\d+)/(-?\d+) us\)")


def parse(line):
    m = LINE.search(line or "")
    if not m:
        return None
    words = m.group(3).split()
    parts = {words[i]: int(words[i + 1]) for i in range(0, len(words) - 1, 2)}
    return {
        "lit": m.group(1) == "at",
        "total": int(m.group(2)),
        "parts": parts,
        "lead": int(m.group(4)),
        "lead_end": int(m.group(5)),
    }


def cell(values):
    if not values:
        return "—"
    med = statistics.median(values)
    lo, hi = min(values), max(values)
    med_s = f"{med:g}"
    return med_s if lo == hi else f"{med_s} ({lo}–{hi})"


def table(title, groups):
    print(f"\n### {title}\n")
    print("| case | n | total | " + " | ".join(PARTS) + " | compile | lp-sys at entry / at the line (µs) |")
    print("|---|---:|---:|" + "---:|" * len(PARTS) + "---:|---|")
    for case, boots in groups:
        ps = [p for p in (parse(b.get("line")) for b in boots) if p]
        if not ps:
            continue
        verdict = "" if ps[0]["lit"] else " (none lit)"
        compiles = []
        for b in boots:
            m = re.search(r"elapsed=(\d+)ms", b.get("compile") or "")
            if m:
                compiles.append(int(m.group(1)))
        row = [f"{case}{verdict}", str(len(ps)), f"**{cell([p['total'] for p in ps])}**"]
        row += [cell([p["parts"].get(k, 0) for p in ps]) for k in PARTS]
        row.append(cell(compiles))
        row.append(f"{cell([p['lead'] for p in ps])} / {cell([p['lead_end'] for p in ps])}")
        print("| " + " | ".join(row) + " |")


def main():
    d = Path(sys.argv[1])
    for f in sorted(d.glob("first-light-*.json")):
        rec = json.loads(f.read_text())
        groups = [(r["case"], r["boots"]) for r in rec["rows"]]
        name = f.stem.removeprefix("first-light-")
        table(f"silicon, {name} (CX1)", groups)
    for f in sorted(d.glob("emu-first-light-*.json")):
        rec = json.loads(f.read_text())
        by = {}
        for r in rec["rows"]:
            by.setdefault(r["image"], []).append(r)
        for image, rows in by.items():
            groups = {}
            for r in rows:
                groups.setdefault(r["project"], []).append(r)
            table(f"emulated, {image} ({rec['configuration']})", list(groups.items()))
            pads = [
                (p, cell([round(r["pad_first_lit_start_ms"], 1) for r in rs if r["pad_first_lit_start_ms"]]))
                for p, rs in groups.items()
            ]
            print("\nfirst lit frame's first bit on the pad (ms): " + ", ".join(f"{p} {c}" for p, c in pads))


if __name__ == "__main__":
    main()

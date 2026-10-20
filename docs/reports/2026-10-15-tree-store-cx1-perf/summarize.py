#!/usr/bin/env python3
"""Summarize the tree store's silicon perf sitting on CX1 (M7 scope 2).

    python3 docs/reports/2026-10-15-tree-store-cx1-perf/summarize.py

Reads `perf-<label>.json` and `perf-<label>-console.txt` (written by
`lp-cli/tests/board_tree_store_perf.rs`) for `fs-tree` and `littlefs`, and
prints the report's table: each operation's median round trip, the same less
its baseline, the mounts, and the panel write's auto-save read off the
firmware's `[perf] frames … max=` lines (5 s windows that hold a panel write
against the 60 s idle control after them).
"""

from __future__ import annotations

import json
import os
import re
import statistics

HERE = os.path.dirname(os.path.abspath(__file__))
LABELS = ["fs-tree", "littlefs"]


def frame_windows(label: str, d: dict) -> tuple[list[int], list[int]]:
    """Max frame (ms) of each `[perf] frames` window that ends within 6 s
    after a panel write, and of each window inside the idle control."""
    path = os.path.join(HERE, f"perf-{label}-console.txt")
    panel, idle = [], []
    writes = d["panel_writes_at_s"]
    lo, hi = d["idle_window_s"]
    for line in open(path, encoding="utf-8", errors="replace"):
        m = re.match(r"\s*([\d.]+) .*\[perf\] frames .* max=(\d+)ms", line)
        if not m:
            continue
        t, mx = float(m.group(1)), int(m.group(2))
        if any(0 < t - w <= 6.0 for w in writes):
            panel.append(mx)
        elif lo + 5 < t <= hi:
            idle.append(mx)
    return panel, idle


def main() -> None:
    data = {}
    for label in LABELS:
        p = os.path.join(HERE, f"perf-{label}.json")
        if os.path.exists(p):
            data[label] = json.load(open(p))
    for label, d in data.items():
        b = d["hello"]["build"]
        print(f"{label}: commit {b['commit']} dirty={b['dirty']}, fs at the sitting's start {d['hello']['hardware']['fs']}")
    print()
    print("| operation | " + " | ".join(f"{l} median ms | less baseline" for l in data) + " |")
    print("|---|" + "---:|---:|" * len(data))
    whats = [r["what"] for r in next(iter(data.values()))["rows"]]
    for i, _ in enumerate(whats):
        cells = []
        for d in data.values():
            if i >= len(d["rows"]):
                cells += ["-", "-"]
                continue
            r = d["rows"][i]
            lb = r["less_baseline_ms"]
            cells += [f"{r['median_ms']:.1f} (n={r['n']})", "-" if lb is None else f"{lb:.1f}"]
        print(f"| {data[next(iter(data))]['rows'][i]['what']} | " + " | ".join(cells) + " |")
    print()
    for label, d in data.items():
        by = {}
        for m in d["mounts"]:
            key = re.sub(r" #\d+$", "", m["what"])
            by.setdefault(key, []).append(m["mount_us"])
        for key, v in by.items():
            v = [x for x in v if x is not None]
            if v:
                print(f"{label} {key}: median {statistics.median(v)/1000:.2f} ms (n={len(v)}, {min(v)/1000:.2f}-{max(v)/1000:.2f})")
        fs_lines = [m["fs_line"] for m in d["mounts"] if m.get("fs_line")]
        if fs_lines:
            print(f"{label} last boot line: {fs_lines[-1]}")
        panel, idle = frame_windows(label, d)
        if panel and idle:
            print(
                f"{label} panel auto-save: windows holding a write max frame median {statistics.median(panel)} ms "
                f"({panel}); idle control median {statistics.median(idle)} ms (n={len(idle)}, {min(idle)}-{max(idle)})"
            )
        print()


if __name__ == "__main__":
    main()

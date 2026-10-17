#!/usr/bin/env python3
"""Summarize the tree store's CX1 power-cut soak (M7 scope 1).

    python3 docs/reports/2026-10-15-tree-store-cx1-soak/summarize.py [cycles.jsonl[.gz]]

Reads the soak driver's per-cycle records (`lp-cli/tests/board_tree_store_soak.rs`,
one JSON object per cycle) and prints every figure the M7 report quotes:
cycles, failures, power-on boots, mount verdicts, how each operation landed,
where the cuts fell, the store's own boot line and the flash checks.
"""

from __future__ import annotations

import gzip
import json
import os
import re
import statistics
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))


def load(path: str) -> list[dict]:
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt") as f:
        return [json.loads(line) for line in f if line.strip()]


def pct(a: int, b: int) -> str:
    return f"{100.0 * a / b:.1f} %" if b else "-"


def main() -> None:
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "cycles.jsonl.gz")
    rs = load(path)
    n = len(rs)
    fails = [r for r in rs if r["failures"]]
    print(f"cycles: {n} (cycle {rs[0]['cycle']}..{rs[-1]['cycle']})")
    print(f"failing cycles: {len(fails)}")
    for r in fails:
        print(f"  cycle {r['cycle']} {r['op']}: {r['failures'][:3]}")

    # Every cut a cut: the board said power-on (its own boot line, else a
    # heartbeat's reset reason).
    def power_on(r: dict) -> bool | None:
        if r.get("boot_line"):
            return "cause=power-on" in r["boot_line"]
        if r.get("heartbeat_reset_reason"):
            return r["heartbeat_reset_reason"] == "power-on"
        return None

    po = Counter(power_on(r) for r in rs)
    by_line = sum(1 for r in rs if r.get("boot_line"))
    print(
        f"power-on boots: {po[True]} of {n} (by the board's boot line {by_line}, "
        f"by heartbeat only {po[True] - sum(1 for r in rs if r.get('boot_line') and 'power-on' in r['boot_line'])}); "
        f"not power-on {po[False]}, unknown {po[None]}"
    )
    pcs = [r["power_cycle"] for r in rs if r.get("power_cycle")]
    print(
        f"`board power-cycle` runs: {len(pcs)}, exit 0: {sum(1 for p in pcs if p['code'] == 0)}"
    )
    print(f"mount verdicts: {dict(Counter(r['fs'] for r in rs))}")
    retries = [r for r in rs if r.get("readback_retries")]
    print(f"read-backs retried on a fresh link: {len(retries)}")

    # How each operation landed, and whether the cut caught it unanswered.
    print("\n| operation | cycles | answered before the cut | cut while in flight | landed old | landed new | same/mixed |")
    print("|---|---:|---:|---:|---:|---:|---:|")
    tot = Counter()
    ops = defaultdict(list)
    for r in rs:
        ops[r["op"]].append(r)
    for op in ["push", "push-c40", "save", "panel", "switch"]:
        rows = ops.get(op, [])
        if not rows:
            continue
        answered = sum(1 for r in rows if r["op_result"] == "ok")
        inflight = len(rows) - answered
        # The first event is the operation itself (a push's startup write
        # is a second, board-made event).
        land = Counter(r["landed"][0] if r["landed"] else "None" for r in rows)
        old, new = land["Some(Old)"], land["Some(New)"]
        same = len(rows) - old - new
        print(f"| {op} | {len(rows)} | {answered} | {inflight} | {old} | {new} | {same} |")
        tot.update(cycles=len(rows), answered=answered, inflight=inflight, old=old, new=new, same=same)
    print(
        f"| **all** | **{tot['cycles']}** | {tot['answered']} | {tot['inflight']} | {tot['old']} | {tot['new']} | {tot['same']} |"
    )

    # Unanswered operations that landed new: the board committed what the
    # host never heard about (the cut fell between the commit and the answer).
    late = Counter(
        r["op"] for r in rs if r["op_result"] != "ok" and r["landed"] and r["landed"][0] == "Some(New)"
    )
    print(f"\ncut in flight but landed new (committed, answer lost): {dict(late)}")

    # Where the cuts fell, against the operation's start (the estimate is the
    # command's start + its measured latency).
    cut = [r["cut_est_after_op_start_s"] for r in rs if r.get("cut_est_after_op_start_s") is not None]
    if cut:
        qs = statistics.quantiles(cut, n=10)
        print(f"cut estimate after the operation's start, s: deciles {[round(q, 2) for q in qs]}")

    # The store's own boot line.
    free, root, sectors = [], [], Counter()
    for r in rs:
        m = re.search(r"\((\d+) sectors, (\d+) free, root (\d+)\)", r.get("fs_line") or "")
        if m:
            sectors[int(m.group(1))] += 1
            free.append(int(m.group(2)))
            root.append(int(m.group(3)))
    print(
        f"\n[FS] boot lines: {len(free)} of {n}; sectors {dict(sectors)}; free sectors "
        f"min {min(free)} median {statistics.median(free)} max {max(free)}; root seq "
        f"{root[0]} → {root[-1]}"
    )
    mounts = [r["mount_us"] for r in rs if r.get("mount_us")]
    if mounts:
        big = [r["mount_us"] for r in rs if r.get("mount_us") and r["files"] > 60]
        small = [r["mount_us"] for r in rs if r.get("mount_us") and r["files"] <= 60]
        def s(v: list[int]) -> str:
            return f"median {statistics.median(v)/1000:.2f} ms (min {min(v)/1000:.2f}, max {max(v)/1000:.2f}, n={len(v)})" if v else "-"
        print(f"[perf] fs mount: with c40 on the board {s(big)}; two basic projects {s(small)}")

    flash = [r["flash"] for r in rs if isinstance(r.get("flash"), dict)]
    ok = sum(1 for f in flash if f.get("fsck") and f.get("same"))
    print(f"flash checks (raw lpfs on the host): {len(flash)}, fsck + same files: {ok}")
    secs = [r["cycle_s"] for r in rs]
    print(f"cycle time: median {statistics.median(secs):.1f} s, total {sum(secs)/3600:.2f} h")


if __name__ == "__main__":
    main()

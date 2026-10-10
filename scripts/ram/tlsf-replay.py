#!/usr/bin/env python3
"""E6: TLSF against first fit, on the emulated C6's own allocation traces.

Two modes, both over `lp-cli emu run --alloc-trace` output
(`lp-emu/esp/lp-emu-esp32c6/src/alloc_trace.rs`):

`validate TRACE --elf p2.elf [--fllen N]` — the model against a TLSF image's
own trace. Every allocation of the boot (probes and failed ones included: on
a real TLSF heap they move blocks too) is placed by `tlsf_model.TlsfRegion`
in esp-alloc's region order and compared with the address the device
returned; a failed (null) allocation must fail in the model too. Alignment is
not in the trace, so a block that does not reproduce at align 4 is retried at
16, 32, …; a block nothing reproduces is forced where the trace put it (and
counted). The heartbeat's `largestFreeBlock` (the firmware's probe) is checked
against the model's `largest_request()` where the console line arrived.

`replay TRACE --elf p2.elf [--regions main=186848,dram2=65536,radio=49152]
[--fllen N] [--out PREFIX]` — the counterfactual on a first-fit image's trace.
The trace is read as `lifetime-census.py` reads it (probe allocations through
`largest_free_block` dropped, failed ones dropped), today's first fit is
replayed with its alignment inference (`replay_today`), and the same
allocations and frees, in the same order, go through the TLSF model with the
alignment first fit needed. At every key marker (compile start, `[perf]` line,
load/unload/stop edges: E9's set) it records, per algorithm: the largest hole,
the largest request the probe would get, free bytes, and the live set's
footprint. A request no TLSF region can serve is skipped and counted (its free
too), so every figure after the first one is optimistic for TLSF.

Standard library only; `rust-nm` on PATH.
"""

from __future__ import annotations

import argparse
import csv
import importlib.util
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import tlsf_model as tm  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "lifetime_census", Path(__file__).with_name("lifetime-census.py"))
lc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lc)
atr = lc.atr

# esp-alloc's registration order and capability tags (fw-esp32c6 init.rs).
ORDER = (("main", lc.INTERNAL), ("dram2", 0), ("radio", lc.INTERNAL | lc.EXTERNAL))
LLFF_BASES = {"main": 0x40827028, "dram2": 0x4086E610, "radio": 0x4081B028}
KEY_MARKS = ("compilation starting", "[perf] frame=", "[mem] load_project after",
             "[mem] boot auto_load after", "[mem] load_project unload existing before",
             "[mem] stop_all_projects before")
ALIGNS = (16, 32, 64, 128, 256, 512, 1024, 2048, 4096)


def regions_from_elf(elf: str):
    found = {nm: (base, size) for nm, base, size in atr.heap_regions(elf)}
    return [(nm, found[nm][0], found[nm][1], tag) for nm, tag in ORDER]


# --------------------------------------------------------------------------
# validate


def validate(args) -> int:
    sym = atr.Symbolizer(args.elf)
    regs = regions_from_elf(args.elf)
    heap = [tm.TlsfRegion(nm, base, size, args.fllen) for nm, base, size, _ in regs]
    tags = [tg for *_, tg in regs]
    print(f"regions ({args.fllen=}): " + ", ".join(
        f"{nm} {base:#x} {size:,} B (pool {h.bottom:#x}..{h.top:#x})"
        for (nm, base, size, _), h in zip(regs, heap)))
    stats = defaultdict(int)
    first_bad = []
    where: dict[int, int] = {}
    hb = []  # (model largest_request, firmware largestFreeBlock, model per region)
    probe_of: dict[str, bool] = {}
    boot = 0
    n = 0
    with open(args.trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                if boot != args.boot:
                    continue
                _, cyc, ptr, size, caps, frames = line.split(" ", 5)
                ptr, size, caps = int(ptr, 16), int(size), int(caps)
                is_probe = probe_of.get(frames)
                if is_probe is None:
                    is_probe = any(lc.PROBE_SITE in sym(int(x, 16))
                                   for x in frames.strip().split(",") if x)
                    probe_of[frames] = is_probe
                n += 1
                order = [i for i, tg in enumerate(tags) if tg & caps == caps]
                if ptr == 0:
                    hit = next((heap[i].find(size, 4) for i in order
                                if heap[i].find(size, 4) is not None), None)
                    stats["failed-and-model-failed" if hit is None
                          else "failed-but-model-served"] += 1
                    if hit is not None and len(first_bad) < 10:
                        first_bad.append(f"A {cyc} null {size} caps {caps}: model would serve")
                    continue
                placed = False
                for i in order:
                    hit = heap[i].find(size, 4)
                    if hit is None:
                        continue
                    if hit[1] == ptr:
                        heap[i].commit(hit)
                        where[ptr] = i
                        placed = True
                        stats["reproduced" + ("-probe" if is_probe else "")] += 1
                    break
                if placed:
                    continue
                ri = next((i for i, h in enumerate(heap) if h.contains(ptr)), None)
                if ri is None:
                    stats["outside-every-region"] += 1
                    continue
                for al in ALIGNS:
                    hit = heap[ri].find(size, al)
                    if hit is not None and hit[1] == ptr:
                        heap[ri].commit(hit)
                        where[ptr] = ri
                        placed = True
                        stats[f"reproduced-align-{al}"] += 1
                        break
                if not placed:
                    if len(first_bad) < 10:
                        model = [heap[i].find(size, 4) for i in order]
                        first_bad.append(
                            f"A {cyc} {ptr:#x} {size} caps {caps}: model "
                            + ", ".join(f"{heap[i].name}:{m[1]:#x}" if m else f"{heap[i].name}:none"
                                        for i, m in zip(order, model)))
                    if heap[ri].carve(ptr, size):
                        where[ptr] = ri
                        stats["forced"] += 1
                    else:
                        stats["collision"] += 1
            elif tag == "F":
                if boot != args.boot:
                    continue
                _, cyc, ptr, _size = line.split(" ", 3)
                ri = where.pop(int(ptr, 16), None)
                if ri is None:
                    stats["unmatched-free"] += 1
                    continue
                heap[ri].deallocate(int(ptr, 16))
            elif tag in "LM":
                _, cyc, text = line.rstrip("\n").split(" ", 2)
                if text.startswith("@reboot"):
                    boot += 1
                    continue
                if tag == "M" and boot == args.boot and "largestFreeBlock" in text:
                    m = re.search(r'"largestFreeBlock":(\d+)', text)
                    if m:
                        hb.append((max(h.largest_request() for h in heap), int(m.group(1)),
                                   [h.largest_request() for h in heap]))
    for h in heap:
        h.check()
    rep = sum(v for k, v in stats.items() if k.startswith("reproduced"))
    succ = n - stats["failed-and-model-failed"] - stats["failed-but-model-served"]
    print(f"{n:,} allocations in boot {args.boot} ({succ:,} served by the device)")
    print(f"placements reproduced: {rep:,} of {succ:,} ({100 * rep / max(succ, 1):.4f} %)")
    for k, v in sorted(stats.items()):
        print(f"  {k}: {v:,}")
    same = sum(1 for m, fw, _ in hb if m == fw)
    print(f"heartbeat largestFreeBlock vs model largest_request: {same} of {len(hb)} equal; "
          + ", ".join(f"{fw:,}/{m:,}" for m, fw, _ in hb[:12]))
    for b in first_bad:
        print("  first mismatches:", b)
    if args.json:
        Path(args.json).write_text(json.dumps({
            "trace": args.trace, "elf": args.elf, "fllen": args.fllen, "allocations": n,
            "served": succ, "reproduced": rep, "stats": dict(stats),
            "heartbeats": [{"firmware": fw, "model": m, "per_region": pr} for m, fw, pr in hb],
            "first_mismatches": first_bad}, indent=1))
    return 0


# --------------------------------------------------------------------------
# replay (the counterfactual)


def replay(args) -> int:
    sym = atr.Symbolizer(args.elf)
    t = lc.parse(args.trace, args.boot, sym)
    n = len(t.size)
    _, align, stats, snaps, where_ff = lc.replay_today(t, lc.C6_REGIONS)
    rep = sum(v for k, v in stats.items() if k.startswith("reproduced"))
    sizes = dict(kv.split("=") for kv in args.regions.split(","))
    regs = [(nm, LLFF_BASES[nm], int(sizes[nm]), tg) for nm, tg in ORDER]
    heap = [tm.TlsfRegion(nm, base, size, args.fllen) for nm, base, size, _ in regs]
    tags = [tg for *_, tg in regs]
    where = [None] * n
    # First fit again, on THESE regions (equal to today's replay when the
    # regions are today's), so every configuration has a like-for-like row.
    ffh = [lc.Region(nm, base, size) for nm, base, size, _ in regs]
    ff_where = [None] * n
    ff_ptr = [0] * n
    ff_oom = 0
    oom = 0
    first_oom = None
    marks_at = defaultdict(list)
    for mi, (ei, _, _) in enumerate(t.marks):
        marks_at[ei].append(mi)
    # First fit's live footprint, walked alongside (footprint of what it placed).
    ff_live = 0
    ff_fp = [lc.Region.footprint(s) for s in t.size]
    live_req = 0
    rows = []

    def snapshot(mi):
        cyc, text = t.marks[mi][1], t.marks[mi][2]
        if not any(k in text for k in KEY_MARKS):
            return
        ff = snaps.get(mi)
        rows.append({
            "mark": mi, "cycle": cyc, "s": round(cyc / lc.CYCLES_PER_US / 1e6, 3),
            "text": text[:90],
            "ff_largest": max(x[0] for x in ff), "ff_free": sum(x[1] for x in ff),
            "ff_largest_by_region": [x[0] for x in ff],
            "tlsf_hole": max(h.largest_hole() for h in heap),
            "tlsf_request": max(h.largest_request() for h in heap),
            "tlsf_free": sum(h.free() for h in heap),
            "tlsf_hole_by_region": [h.largest_hole() for h in heap],
            "tlsf_request_by_region": [h.largest_request() for h in heap],
            "tlsf_used": sum(h.used_bytes() - tm.GRAN * 1 for h in heap),
            "ff_live": ff_live, "requested_live": live_req, "oom_so_far": oom,
            "ffl_largest": max(h.largest() for h in ffh), "ffl_free": sum(h.free() for h in ffh),
            "ffl_oom_so_far": ff_oom,
        })

    for ei, ev in enumerate(t.events):
        for mi in marks_at.get(ei, ()):
            snapshot(mi)
        r, is_free = ev >> 1, ev & 1
        if is_free:
            if where_ff[r] is not None:
                ff_live -= ff_fp[r]
            live_req -= t.size[r]
            ri = where[r]
            if ri is not None:
                heap[ri].deallocate(where_ptr[r])
            fi = ff_where[r]
            if fi is not None:
                ffh[fi].release(ff_ptr[r], t.size[r])
            continue
        if where_ff[r] is not None:
            ff_live += ff_fp[r]
        live_req += t.size[r]
        caps = t.caps[r]
        for i, tg in enumerate(tags):
            if tg & caps != caps:
                continue
            hit = ffh[i].find(t.size[r], max(align[r], 4))
            if hit is not None:
                ff_ptr[r] = ffh[i].commit(hit)
                ff_where[r] = i
                break
        else:
            ff_oom += 1
        for i, tg in enumerate(tags):
            if tg & caps != caps:
                continue
            p = heap[i].allocate(t.size[r], max(align[r], 4))
            if p is not None:
                where[r] = i
                where_ptr[r] = p
                break
        else:
            oom += 1
            if first_oom is None:
                first_oom = (ei, t.born[r], t.size[r], caps,
                             [h.largest_request() for h in heap], [h.free() for h in heap])
    for mi in marks_at.get(len(t.events), ()):
        snapshot(mi)
    for h in heap:
        h.check()

    first_load = min((c for _, c, x in t.marks
                      if "Loading project:" in x or "[mem] boot auto_load before" in x),
                     default=0)
    after = [row for row in rows if row["cycle"] >= first_load]

    def worst(key):
        return min((row[key] for row in after), default=0)

    summary = {
        "trace": args.trace, "regions": args.regions, "fllen": args.fllen,
        "allocations": n, "first_fit_reproduced": rep, "markers": len(rows),
        "markers_after_first_load": len(after), "tlsf_would_oom": oom,
        "first_oom": None if first_oom is None else {
            "event": first_oom[0], "s": round(first_oom[1] / lc.CYCLES_PER_US / 1e6, 3),
            "size": first_oom[2], "caps": first_oom[3],
            "request_by_region": first_oom[4], "free_by_region": first_oom[5]},
        "worst_ff_largest": worst("ff_largest"), "worst_tlsf_hole": worst("tlsf_hole"),
        "worst_ff_same_regions": worst("ffl_largest"), "ff_same_regions_would_oom": ff_oom,
        "worst_tlsf_request": worst("tlsf_request"),
        "median_ratio_request": median([row["tlsf_request"] / row["ff_largest"]
                                        for row in after if row["ff_largest"]]),
        "peak_ff_live": max((row["ff_live"] for row in rows), default=0),
        "peak_tlsf_used": max((row["tlsf_used"] for row in rows), default=0),
        "max_overhead": max((row["tlsf_used"] - row["ff_live"] for row in rows), default=0),
        "median_overhead": median([row["tlsf_used"] - row["ff_live"] for row in after]),
    }
    print(json.dumps(summary, indent=1))
    if args.out:
        Path(args.out + ".json").write_text(json.dumps({"summary": summary, "rows": rows}, indent=0))
        with open(args.out + ".csv", "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["s", "mark", "ff_today_largest", "ff_these_regions_largest",
                        "tlsf_hole", "tlsf_request", "ff_today_free", "tlsf_free", "ff_live",
                        "tlsf_used", "tlsf_oom_so_far", "ff_these_regions_oom_so_far", "text"])
            for row in rows:
                w.writerow([row["s"], row["mark"], row["ff_largest"], row["ffl_largest"],
                            row["tlsf_hole"], row["tlsf_request"], row["ff_free"],
                            row["tlsf_free"], row["ff_live"], row["tlsf_used"],
                            row["oom_so_far"], row["ffl_oom_so_far"], row["text"]])
    return 0


where_ptr: dict[int, int] = {}


def median(xs):
    xs = sorted(xs)
    if not xs:
        return None
    return round(xs[len(xs) // 2], 4)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="mode", required=True)
    v = sub.add_parser("validate")
    v.add_argument("trace")
    v.add_argument("--elf", required=True)
    v.add_argument("--boot", type=int, default=0)
    v.add_argument("--fllen", type=int, default=32)
    v.add_argument("--json")
    r = sub.add_parser("replay")
    r.add_argument("trace")
    r.add_argument("--elf", required=True)
    r.add_argument("--boot", type=int, default=0)
    r.add_argument("--fllen", type=int, default=32)
    r.add_argument("--regions", default="main=186848,dram2=65536,radio=49152")
    r.add_argument("--out")
    args = ap.parse_args()
    return validate(args) if args.mode == "validate" else replay(args)


if __name__ == "__main__":
    sys.exit(main())

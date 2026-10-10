#!/usr/bin/env python3
"""E15: price "keep the per-edit results out of `dram2_seg`" on a real trace.

Replays one boot of an emulated C6's allocation trace (`lp-cli emu run
--alloc-trace`) on today's three regions — main, `dram2_seg`, radio, tried in
that order by capability, `linked_list_allocator` first fit at RV32 geometry
(lifetime-census.py's `Region`, which reproduces E9's 7.9 M placements) — and
then again under a routing policy: an allocation whose stack passes through a
**scope** frame tries `dram2_seg` LAST (main, then radio, then `dram2_seg`)
instead of second. Everything else is placed as today.

Each allocation's alignment is not in the trace; the first (today) replay
infers it as the smallest that reproduces the recorded address, and the
policy replay reuses it.

At every `[mem] shader compile before` record it reports, per policy: each
region's largest hole, total free, and allocations the policy could not place
(the model's OOM: a real image would have reset or refused). Radio C asks
(`caps != 0`) that no longer fit the radio region are counted separately —
on silicon they fall back to main.

Usage:
    e15-route-replay.py TRACE --elf p2.elf [--scope REGEX]… [--boot N]

The default scopes are the three per-edit owners E9/E11 named:
`refresh_artifacts` (the registry inventory), `materialize_asset_text` (the
shader text) and `link_compiled_module_jit` (the linked JIT module).
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import statistics
import sys
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "lifetime_census", Path(__file__).with_name("lifetime-census.py"))
lc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lc)
atr = lc.atr

DEFAULT_SCOPES = (
    r"ProjectRegistry>::refresh_artifacts",
    r"ProjectRegistry>::materialize_asset_text",
    r"rt_jit::compiler::link_compiled_module_jit",
)
MARK = "[mem] shader compile before"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--boot", type=int, default=0)
    ap.add_argument("--scope", action="append", default=[],
                    help="regex on a demangled frame; default: the three per-edit owners")
    ap.add_argument("--order", default="main,radio,dram2",
                    help="region order for a scoped allocation (default main,radio,dram2)")
    ap.add_argument("--map", type=int, action="append", default=[], metavar="N",
                    help="also print dram2's and radio's live runs at compile start N (both replays)")
    ap.add_argument("--residents-only-ms", type=float, default=None, metavar="MS",
                    help="ORACLE: route only the scoped blocks that live longer than MS "
                         "(what tagging the result sites alone, not their whole scope, would do)")
    ap.add_argument("--exclude", action="append", default=[], metavar="REGEX",
                    help="a stack with a frame matching this is never scoped (e.g. a file read "
                         "inside the scope)")
    ap.add_argument("--hoist", action="store_true",
                    help="residents-first instead of routing: each scoped block living > 100 ms "
                         "is allocated at the start of its scope's invocation (a run of scoped "
                         "events with < 200 other events between them), today's region order")
    ap.add_argument("--split", type=int, default=0, metavar="BYTES",
                    help="policy layout: dram2_seg's top BYTES as a region of their own "
                         "(`dram2hi`, reachable by unscoped requests after the low part); "
                         "name it in --order")
    args = ap.parse_args()
    scopes = [re.compile(s) for s in (args.scope or DEFAULT_SCOPES)]

    sym = atr.Symbolizer(args.elf)
    t = lc.parse(args.trace, args.boot, sym)
    elf_regions = {n: (s, z) for n, s, z in atr.heap_regions(args.elf)}
    spec = [(n, z, tg) for n, z, tg in lc.C6_REGIONS]
    bases = {n: elf_regions[n][0] for n, _, _ in spec}

    excludes = [re.compile(s) for s in args.exclude]
    scoped_fid: dict[int, bool] = {}

    def scoped_raw(r: int) -> bool:
        fid = t.frames[r]
        hit = scoped_fid.get(fid)
        if hit is None:
            names = [sym(int(x, 16)) for x in t.frame_strings[fid].split(",") if x]
            hit = any(p.search(n) for p in scopes for n in names) and not any(
                p.search(n) for p in excludes for n in names)
            scoped_fid[fid] = hit
        return hit

    def resident(r: int, ms: float = 100.0) -> bool:
        died = t.died[r] if t.died[r] is not None else t.end_cycle
        return died - t.born[r] > ms * 1000 * lc.CYCLES_PER_US

    def scoped(r: int) -> bool:
        hit = scoped_raw(r)
        if hit and args.residents_only_ms is not None:
            hit = resident(r, args.residents_only_ms)
        return hit

    pspec, pbases = spec, dict(bases)
    if args.split:
        # dram2_seg as two regions: the low one is the big block, the top
        # `--split` bytes the long-lived results' (adjacent, as a firmware
        # with two esp-alloc regions there would have them).
        pspec = [("main", 186_848, lc.INTERNAL), ("dram2", 65_536 - args.split, 0),
                 ("dram2hi", args.split, 0), ("radio", 49_152, lc.INTERNAL | lc.EXTERNAL)]
        pbases["dram2hi"] = bases["dram2"] + 65_536 - args.split
    names = [n for n, _, _ in pspec]
    scoped_order = [names.index(n) for n in args.order.split(",")]
    map_at = set(args.map)
    align, today = replay(t, spec, bases, infer=True, map_at=map_at)
    if args.hoist:
        # Residents-first: no routing change; each scoped resident is allocated
        # at the start of its scope's invocation, ahead of the scope's scratch.
        t.events = hoisted_events(t, scoped_raw, resident)
        _, policy = replay(t, pspec, pbases, align=align, map_at=map_at)
    else:
        _, policy = replay(t, pspec, pbases, align=align, scoped=scoped, scoped_order=scoped_order,
                           map_at=map_at)

    n_scoped = sum(1 for r in range(len(t.size)) if scoped(r))
    b_scoped = sum(t.size[r] for r in range(len(t.size)) if scoped(r))
    print(f"# e15-route-replay: {args.trace}")
    print(f"# scopes: {', '.join(p.pattern for p in scopes)}; scoped order {args.order}")
    print(f"# {len(t.size):,} allocations, {n_scoped:,} scoped ({b_scoped:,} B); "
          f"today's replay: {dict(today['stats'])}")
    print(f"# policy replay: {dict(policy['stats'])}")
    print()
    print(f"| compile | today: {' / '.join(n for n, _, _ in spec)} largest | today free | "
          f"policy: {' / '.join(names)} largest | policy free | policy OOM so far | radio C moved to main so far |")
    print("|---:|---|---:|---|---:|---:|---:|")
    lt, lp = [], []
    for i, (a, b) in enumerate(zip(today["snaps"], policy["snaps"])):
        lt.append(a["dram2"])
        lp.append(b["dram2"])
        fa = sum(f for _, f in a["regions"])
        fb = sum(f for _, f in b["regions"])
        print(f"| {i + 1} | {' / '.join(f'{x:,}' for x, _ in a['regions'])} | {fa:,} "
              f"| {' / '.join(f'{x:,}' for x, _ in b['regions'])} | {fb:,} | {b['oom']} | {b['radio_c_to_main']} |")
    print()
    for label, xs in (("today", lt), ("policy", lp)):
        edits = xs[2:12] if len(xs) >= 12 else xs
        print(f"dram2 largest at compile starts, {label}: all min {min(xs):,} / median "
              f"{int(statistics.median(xs)):,} / max {max(xs):,}; edits (3..12) min {min(edits):,} / "
              f"median {int(statistics.median(edits)):,}")
    print(f"policy: {policy['oom']} allocation(s) the model could not place; "
          f"{policy['radio_c_to_main']} radio C ask(s) that no longer fit the radio region "
          f"({policy['radio_c_to_main_bytes']:,} B)")
    def owner(q: int) -> str:
        chain = [atr.HASH_SUFFIX.sub("", sym(int(x, 16))) for x in t.frame_strings[t.frames[q]].split(",") if x]
        chain = [c for c in chain if not atr.is_machinery(c)]
        return chain[0] if chain else "?"

    for n in sorted(map_at):
        for label, res in (("today", today), ("policy", policy)):
            if n > len(res["snaps"]):
                continue
            for region in ("dram2", "radio"):
                print(f"\n{label}, compile start {n}, {region} (Rust blocks; scoped marked *):")
                runs: list[list] = []
                last = None
                for off, size, q in res["snaps"][n - 1]["live"][region]:
                    if t.caps[q] != 0:
                        continue
                    if last is None or off - last >= 64:
                        runs.append([])
                    runs[-1].append((off, size, q))
                    last = max(last or 0, off + size)
                for run in runs:
                    big = max(run, key=lambda x: x[1])
                    mark = "*" if scoped(big[2]) else " "
                    print(f"  +{run[0][0]}..+{max(o + s for o, s, _ in run)}  {sum(s for _, s, _ in run):>6} B "
                          f"in {len(run):>3}  {mark} {owner(big[2])[:100]} ({big[1]} B)")
    if policy["oom_sites"]:
        print("first unplaceable allocations:")
        for size, fid in policy["oom_sites"][:5]:
            chain = [atr.HASH_SUFFIX.sub("", sym(int(x, 16))) for x in t.frame_strings[fid].split(",") if x]
            chain = [c for c in chain if not atr.is_machinery(c)][:4]
            print(f"  {size} B  {' < '.join(chain)}")
    return 0


def hoisted_events(t, scoped_raw, resident, gap: int = 200) -> list[int]:
    """The event list with every scoped resident's allocation moved to the
    first event of its invocation (residents-first, as `lp-cli profile --cf
    residents-first` does for a marker window)."""
    events = list(t.events)
    moves: dict[int, list[int]] = {}  # cluster start index -> resident alloc events
    drop: set[int] = set()
    start = last = None
    for ei, ev in enumerate(events):
        r = ev >> 1
        if not scoped_raw(r):
            continue
        if last is None or ei - last > gap:
            start = ei
        last = ei
        if not ev & 1 and resident(r) and ei != start:
            moves.setdefault(start, []).append(ev)
            drop.add(ei)
    out = []
    for ei, ev in enumerate(events):
        if ei in moves:
            out.extend(moves[ei])
        if ei not in drop:
            out.append(ev)
    return out


def replay(t, spec, bases, infer=False, align=None, scoped=None, scoped_order=None, map_at=()):
    regions = [lc.Region(n, bases[n], z) for n, z, _ in spec]
    tags = [tg for _, _, tg in spec]
    al = list(align) if align else [4] * len(t.size)
    where = [None] * len(t.size)
    addr = [0] * len(t.size)
    stats: dict[str, int] = {}
    out = {"snaps": [], "oom": 0, "oom_sites": [], "radio_c_to_main": 0, "radio_c_to_main_bytes": 0}
    marks_at: dict[int, list[str]] = {}
    for ei, _, text in t.marks:
        marks_at.setdefault(ei, []).append(text)

    def bump(k):
        stats[k] = stats.get(k, 0) + 1

    def snap():
        n = len(out["snaps"]) + 1
        s = {"regions": [(rg.largest(), rg.free()) for rg in regions],
             "oom": out["oom"], "radio_c_to_main": out["radio_c_to_main"]}
        # dram2_seg's largest free run, across the split if there is one.
        lo = next(rg for rg in regions if rg.name == "dram2")
        hi = next((rg for rg in regions if rg.name == "dram2hi"), None)
        best = lo.largest()
        if hi is not None:
            best = max(best, hi.largest())
            if lo.st and hi.st and lo.st[-1] + lo.sz[-1] == lo.top and hi.st[0] == hi.bottom:
                best = max(best, lo.sz[-1] + hi.sz[0])
        s["dram2"] = best
        if n in map_at:
            s["live"] = {name: sorted((addr[q] - regions[i].bottom, t.size[q], q)
                                      for q in live_set if where[q] == i)
                         for i, name in enumerate(rg.name for rg in regions)}
        out["snaps"].append(s)

    live_set: set[int] = set()

    for ei, ev in enumerate(t.events):
        for text in marks_at.get(ei, ()):
            if MARK in text:
                snap()
        r, is_free = ev >> 1, ev & 1
        if is_free:
            ri = where[r]
            if ri is not None:
                regions[ri].release(addr[r], t.size[r])
                live_set.discard(r)
            continue
        caps = t.caps[r]
        order = [i for i, tg in enumerate(tags) if tg & caps == caps]
        if infer:
            want = t.ptr[r]
            ri = next((i for i, rg in enumerate(regions) if rg.contains(want)), None)
            if ri is None:
                bump("outside-every-region")
                continue
            placed = False
            for a in (4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096):
                hit = regions[ri].find(t.size[r], a)
                if hit is not None and hit[1] == want:
                    regions[ri].commit(hit)
                    al[r] = a
                    placed = True
                    bump("reproduced" if a == 4 else "reproduced-align")
                    break
            if not placed:
                if regions[ri].carve(want, t.size[r]):
                    bump("forced")
                else:
                    bump("collision")
                    continue
            where[r], addr[r] = ri, want
            live_set.add(r)
            continue
        if scoped is not None and caps == 0 and scoped(r):
            order = [i for i in scoped_order if i in order]
        for i in order:
            hit = regions[i].find(t.size[r], al[r])
            if hit is None:
                continue
            addr[r] = regions[i].commit(hit)
            where[r] = i
            live_set.add(r)
            if caps != 0 and i != order[0]:
                out["radio_c_to_main"] += 1
                out["radio_c_to_main_bytes"] += t.size[r]
            break
        else:
            out["oom"] += 1
            if len(out["oom_sites"]) < 20:
                out["oom_sites"].append((t.size[r], t.frames[r]))
    out["stats"] = stats
    return al, out


if __name__ == "__main__":
    sys.exit(main())

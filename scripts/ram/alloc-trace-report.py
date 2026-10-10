#!/usr/bin/env python3
"""Attribute an emulated C6's heap between named points of an allocation trace.

The trace is `lp-cli emu run --alloc-trace <file>`'s (format:
`lp-emu/esp/lp-emu-esp32c6/src/alloc_trace.rs`): `A`/`F` lines for every
allocation and free, with return-address backtraces, and `M` marker lines —
one per console line, plus `@reboot` where the chip reset (every live block
is dropped there).

A *point* is `NAME=BOOT:TEXT[#N]`: the N-th (default 1st) marker containing
TEXT after the BOOT-th `@reboot` (0 = power-on). At each point the script
takes the live set; between consecutive points it reports what was born and
still lives (`new`), what lived before and died (`freed`), grouped by owner —
the first frame that is not allocator or container machinery. `--end` adds a
point at the end of the trace.

Markers lag the guest by up to one host slice, so a point is a bound. The
firmware's own `[mem]` line nearest each point is printed beside the trace's
live sum: the allocator rounds each block (8-byte units with a header), so the
trace's requested bytes are a little under the allocator's `used`.

Usage:
    alloc-trace-report.py TRACE --elf p2.elf --at load=1:"boot auto_load after" \\
        --at compile=1:"shader compile before" --end [--top 25] [--inline 12]

`--inline N` runs `addr2line -i` (GNU binutils with riscv support, e.g.
Homebrew's) on the top N owners' call sites, so an inlined owner is named.
Standard library only; `rust-nm` must be on PATH (cargo-binutils).
"""

from __future__ import annotations

import argparse
import bisect
import re
import shutil
import subprocess
import sys
from collections import defaultdict

# Frames that are allocator or container machinery: skipped when picking the
# owner of a block. Matched as substrings of the demangled function name.
MACHINERY = (
    "esp_alloc::",
    "__rust_alloc",
    "__rust_realloc",
    "__rust_dealloc",
    "__rdl_",
    "__rg_",
    "alloc::alloc::",
    "alloc::raw_vec",
    "<alloc::raw_vec",
    "RawVec",
    "RawVecInner",
    "finish_grow",
    "do_reserve",
    "grow_one",
    "grow_amortized",
    "try_allocate_in",
    "allocate_in",
    "hashbrown::raw",
    "<hashbrown::raw",
    "reserve_rehash",
    "alloc::vec::",
    "<alloc::vec::",
    "alloc::boxed::",
    "<alloc::boxed::",
    "alloc::string::",
    "<alloc::string::",
    "alloc::collections::",
    "<alloc::collections::",
    "alloc::fmt::",
    "alloc::sync::",
    "<alloc::sync::",
    "alloc::rc::",
    "<alloc::rc::",
    "alloc::slice::",
    "<alloc::slice::",
    "alloc::str::",
    "core::fmt::",
    "<core::fmt::",
    "<&T as core::fmt",
    "core::iter::",
    "<core::iter::",
    "<T as alloc::",
    "<T as core::convert::Into",
    "alloc::borrow::",
    "<alloc::borrow::",
    "smallvec::",
    "<smallvec::",
)

# Subsystems: (regex on a demangled frame, label). A block's subsystem is the
# INNERMOST frame that matches any rule, so a value snapshot made for the
# resolver counts as the snapshot, and the resolver row is the resolver's own.
SUBSYSTEMS = (
    (r"packed_link|lp_json_pack", "link: JSON Pack (packed replies)"),
    (r"lps_glsl|lpvm_native::rt_jit::compile|lpvm_native::.*compile|naga", "shader compile"),
    (r"NativeHostMemory|alloc_texture|create_texture|create_sample", "shader runtime memory (textures, JIT host memory)"),
    (r"lpc_wire::slot::access_sync|ToLpValue|SlotValueAccess>::value|slot_data::SlotData", "slot value snapshots (lpc_wire access_sync)"),
    (r"dataflow::resolver", "dataflow resolver (intern, cache, sessions)"),
    (r"SlotAccessor|SlotPath|SlotName|slot_shape", "slot paths / accessors / shapes"),
    (r"palette_bake_cache", "palette bake cache"),
    (r"DisplayPipeline|output::provider|ws281x|OutputNode|output_node", "output (display pipeline, ports)"),
    (r"lpc_hardware|button_driver|hw_registry", "hardware registry / drivers"),
    (r"project_loader|lpc_registry|project_registry|ProjectManager>::load|panel_state|lpa_server::project::", "project load (model, registry, loader)"),
    (r"lpc_engine::", "engine (other)"),
    (r"usb_link|link_mux|lp_link::|radio_link", "link transport"),
    (r"esp_radio|trouble_host|fw_esp32c6::ble|c_heap", "radio (BLE / Wi-Fi C heap overflow)"),
    (r"fw_esp32_common::net|fw_esp32c6::net|lp_net|smoltcp|relay", "network (station, LAN, relay)"),
    (r"lpa_server::", "server (other)"),
    (r"littlefs|lpfs|lp_fs", "filesystem"),
)

HASH_SUFFIX = re.compile(r"::h[0-9a-f]{16}$")
MEM_LINE = re.compile(r"\[mem\] (.+?): (\d+) B free / (\d+) B used")
MEM_LINE_K = re.compile(r"\[mem\] (.+?): (\d+)k free / (\d+)k used")


def load_symbols(elf: str):
    out = subprocess.run(
        ["rust-nm", "-n", "-C", "-S", elf], capture_output=True, text=True, check=True
    ).stdout
    addrs, names, ends = [], [], []
    for line in out.splitlines():
        parts = line.split(" ", 3)
        if len(parts) == 4 and parts[2] in "tTwW":
            addr, size, name = int(parts[0], 16), int(parts[1], 16), parts[3]
        elif len(parts) >= 3 and parts[1] in "tTwW":
            addr, size, name = int(parts[0], 16), 0, line.split(" ", 2)[2]
        else:
            continue
        addrs.append(addr)
        names.append(HASH_SUFFIX.sub("", name))
        ends.append(addr + size if size else None)
    return addrs, names, ends


def heap_regions(elf: str):
    """`(name, start, size)` of fw-esp32c6's heap regions, from their statics."""
    out = subprocess.run(
        ["rust-nm", "-S", "-C", elf], capture_output=True, text=True, check=True
    ).stdout
    regions = []
    # Linker-defined spans (`_heap_<name>_start` / `_heap_<name>_end`, absolute
    # symbols with no size): the main region and the reclaimed tail since the
    # stack moved into dram2_seg (RAM research E4, `fw-esp32c6/build.rs`).
    bounds: dict[str, dict[str, int]] = defaultdict(dict)
    for line in out.splitlines():
        m = re.match(r"^([0-9a-f]+) ([0-9a-f]+) [bBdD] .*init::HEAP_([A-Z0-9]+)$", line)
        if m:
            regions.append((m.group(3).lower(), int(m.group(1), 16), int(m.group(2), 16)))
            continue
        m = re.match(r"^([0-9a-f]+) (?:[0-9a-f]+ )?[aA] _heap_([a-z0-9]+)_(start|end)$", line)
        if m:
            bounds[m.group(2)][m.group(3)] = int(m.group(1), 16)
    for name, b in bounds.items():
        if "start" in b and "end" in b and b["end"] > b["start"]:
            regions.append((name, b["start"], b["end"] - b["start"]))
    return sorted(regions, key=lambda r: r[1])


def region_figures(regions, live, recs_size):
    """Per region: requested bytes live, and the largest gap between live
    blocks (an upper bound on the largest free block: the allocator rounds
    each block up a few bytes)."""
    out = []
    for name, start, size in regions:
        end = start + size
        blocks = sorted((ptr, recs_size[r]) for ptr, r in live.items() if start <= ptr < end)
        used = sum(b for _, b in blocks)
        cursor, largest = start, 0
        for ptr, b in blocks:
            largest = max(largest, ptr - cursor)
            cursor = max(cursor, ptr + b)
        largest = max(largest, end - cursor)
        out.append((name, used, largest))
    return out


class Symbolizer:
    def __init__(self, elf: str):
        self.addrs, self.names, self.ends = load_symbols(elf)
        self.cache: dict[int, str] = {}

    def __call__(self, ret: int) -> str:
        pc = ret - 1  # inside the call instruction
        hit = self.cache.get(pc)
        if hit is not None:
            return hit
        i = bisect.bisect_right(self.addrs, pc) - 1
        name = f"?{ret:08x}"
        if i >= 0:
            end = self.ends[i]
            if end is None or pc < end:
                name = self.names[i]
        self.cache[pc] = name
        return name


def is_machinery(name: str) -> bool:
    return any(m in name for m in MACHINERY)


class Point:
    def __init__(self, name: str, boot: int, text: str, nth: int):
        self.name, self.boot, self.text, self.nth = name, boot, text, nth
        self.seen = 0
        self.live: dict[int, int] | None = None  # ptr -> record id
        self.cycle = None
        self.marker = None
        self.mem = None  # (label, free, used) nearest [mem] line at or before


def parse_point(spec: str) -> Point:
    name, rest = spec.split("=", 1)
    boot, text = rest.split(":", 1)
    nth = 1
    m = re.match(r"^(.*)#(\d+)$", text)
    if m:
        text, nth = m.group(1), int(m.group(2))
    return Point(name, int(boot), text, nth)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("trace")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--at", action="append", default=[], type=parse_point)
    ap.add_argument("--end", action="store_true", help="add a point at the end")
    ap.add_argument("--top", type=int, default=25)
    ap.add_argument("--inline", type=int, default=0)
    ap.add_argument("--depth", type=int, default=3, help="owner frames shown per site")
    ap.add_argument("--dump-live", action="append", default=[], metavar="POINT=FILE",
                    help="write the live set at POINT, by subsystem and owner, as TSV "
                         "(compare two traces with scripts/ram/alloc-live-diff.py)")
    ap.add_argument("--marks", choices=("L", "M"), default="L",
                    help="points from guest log records (L, exact; needs an image with "
                         "alloc-trace-marks) or console arrival (M)")
    args = ap.parse_args()

    sym = Symbolizer(args.elf)
    points: list[Point] = list(args.at)
    # Records: id -> (size, frames string, cycle, boot)
    recs_size: list[int] = []
    recs_frames: list[str] = []
    recs_boot: list[int] = []
    live: dict[int, int] = {}
    boot = 0
    last_mem = None
    pending = [p for p in points]
    totals = defaultdict(int)
    failed = []

    with open(args.trace) as f:
        for line in f:
            tag = line[0]
            if tag == "A":
                _, cyc, ptr, size, caps, frames = line.split(" ", 5)
                ptr = int(ptr, 16)
                size = int(size)
                if ptr == 0:
                    failed.append((boot, int(cyc), size, frames.strip()))
                    continue
                rid = len(recs_size)
                recs_size.append(size)
                recs_frames.append(frames.strip())
                recs_boot.append(boot)
                live[ptr] = rid
                totals["allocs"] += 1
            elif tag == "F":
                _, cyc, ptr, size = line.split(" ", 3)
                live.pop(int(ptr, 16), None)
                totals["frees"] += 1
            elif tag in "ML":
                _, cyc, text = line.rstrip("\n").split(" ", 2)
                if text.startswith("@reboot"):
                    boot += 1
                    live = {}
                    continue
                if tag != args.marks:
                    continue
                mm = MEM_LINE.search(text)
                if mm:
                    last_mem = (mm.group(1), int(mm.group(2)), int(mm.group(3)), "B")
                else:
                    mk = MEM_LINE_K.search(text)
                    if mk:
                        last_mem = (mk.group(1), int(mk.group(2)), int(mk.group(3)), "KiB")
                for p in pending:
                    if p.live is None and p.boot == boot and p.text in text:
                        p.seen += 1
                        if p.seen == p.nth:
                            p.live = dict(live)
                            p.cycle = int(cyc)
                            p.marker = text
                            p.mem = last_mem
    if args.end:
        p = Point("end", boot, "(end of trace)", 1)
        p.live = dict(live)
        p.marker = "(end of trace)"
        p.mem = last_mem
        points.append(p)

    missing = [p.name for p in points if p.live is None]
    if missing:
        print(f"points never reached: {', '.join(missing)}", file=sys.stderr)
        return 1

    def owner(frames: str) -> tuple[str, int]:
        rets = [int(x, 16) for x in frames.split(",") if x]
        names = [sym(r) for r in rets]
        for i, n in enumerate(names):
            if not is_machinery(n):
                chain = " < ".join(names[i : i + args.depth])
                return chain, rets[i]
        return " < ".join(names[-args.depth :]) or "?", rets[-1] if rets else 0

    sub_cache: dict[str, str] = {}

    def subsystem_of(rid: int) -> str:
        fr = recs_frames[rid]
        hit = sub_cache.get(fr)
        if hit is None:
            names = [sym(int(x, 16)) for x in fr.split(",") if x]
            hit = "other"
            for n in names:
                label = next((lab for pat, lab in SUBSYSTEMS if re.search(pat, n)), None)
                if label:
                    hit = label
                    break
            sub_cache[fr] = hit
        return hit

    owner_cache: dict[str, tuple[str, int]] = {}

    def owner_of(rid: int) -> tuple[str, int]:
        fr = recs_frames[rid]
        hit = owner_cache.get(fr)
        if hit is None:
            hit = owner(fr)
            owner_cache[fr] = hit
        return hit

    for spec in args.dump_live:
        name, path = spec.split("=", 1)
        p = next((q for q in points if q.name == name), None)
        if p is None:
            print(f"--dump-live: no point named {name}", file=sys.stderr)
            return 1
        rows = defaultdict(lambda: [0, 0])
        for r in p.live.values():
            key = f"{subsystem_of(r)}\t{owner_of(r)[0]}"
            rows[key][0] += recs_size[r]
            rows[key][1] += 1
        with open(path, "w") as out:
            out.write("bytes\tblocks\tsubsystem\towner\n")
            for key, (byt, cnt) in sorted(rows.items(), key=lambda kv: -kv[1][0]):
                out.write(f"{byt}\t{cnt}\t{key}\n")

    print(f"# alloc-trace-report: {args.trace}")
    print(f"# elf {args.elf}; {totals['allocs']} allocations, {totals['frees']} frees, "
          f"{boot} reboot(s), {len(failed)} failed allocation(s)")
    print()
    print("## Points")
    print()
    print("| point | live bytes (requested) | blocks | firmware's nearest [mem] line | marker |")
    print("|---|---:|---:|---|---|")
    for p in points:
        lb = sum(recs_size[r] for r in p.live.values())
        mem = f"{p.mem[0]}: used {p.mem[2]} {p.mem[3]}" if p.mem else "-"
        print(f"| {p.name} | {lb:,} | {len(p.live):,} | {mem} | {p.marker[:90]} |")
    print()
    regions = heap_regions(args.elf)
    if regions:
        print("Per heap region: live (requested) bytes / largest gap between live blocks "
              "(≈ the region's largest free block):")
        print()
        print("| point | " + " | ".join(f"{n} ({sz:,} B)" for n, _, sz in regions) + " |")
        print("|---|" + "---:|" * len(regions))
        for p in points:
            figs = region_figures(regions, p.live, recs_size)
            print(f"| {p.name} | " + " | ".join(f"{u:,} / {g:,}" for _, u, g in figs) + " |")
        print()

    inline_sites: list[int] = []
    for a, b in zip(points, points[1:]):
        before, after = a.live, b.live
        before_ids = set(before.values())
        after_ids = set(after.values())
        new = [r for r in after_ids if r not in before_ids]
        freed = [r for r in before_ids if r not in after_ids]
        nb = sum(recs_size[r] for r in new)
        fb = sum(recs_size[r] for r in freed)
        print(f"## {a.name} → {b.name}: +{nb:,} B born and live ({len(new):,} blocks), "
              f"−{fb:,} B freed ({len(freed):,} blocks), net {nb - fb:+,} B")
        print()
        last_ids = set(points[-1].live.values())
        sub = defaultdict(lambda: [0, 0, 0])  # bytes, blocks, bytes still live at the last point
        for r in new:
            s = subsystem_of(r)
            sub[s][0] += recs_size[r]
            sub[s][1] += 1
            if r in last_ids:
                sub[s][2] += recs_size[r]
        print(f"By subsystem (innermost frame matching a rule in this script's SUBSYSTEMS). "
              f"'live at {points[-1].name}' = of these bytes, still allocated at the last point:")
        print()
        print(f"| subsystem | bytes | % of new | blocks | live at {points[-1].name} |")
        print("|---|---:|---:|---:|---:|")
        for s, (byt, cnt, kept) in sorted(sub.items(), key=lambda kv: -kv[1][0]):
            print(f"| {s} | {byt:,} | {100 * byt / max(nb, 1):.1f} | {cnt:,} | {kept:,} |")
        print()
        groups = defaultdict(lambda: [0, 0, 0, 0])  # bytes, count, max, site
        for r in new:
            o, site = owner_of(r)
            g = groups[o]
            g[0] += recs_size[r]
            g[1] += 1
            g[2] = max(g[2], recs_size[r])
            g[3] = site
        ranked = sorted(groups.items(), key=lambda kv: -kv[1][0])
        cum = 0
        print("| # | bytes | % of new | cum % | blocks | largest | owner (first non-machinery frame < its callers) |")
        print("|---:|---:|---:|---:|---:|---:|---|")
        for i, (o, (byt, cnt, mx, site)) in enumerate(ranked[: args.top], 1):
            cum += byt
            print(f"| {i} | {byt:,} | {100 * byt / max(nb, 1):.1f} | {100 * cum / max(nb, 1):.1f} | "
                  f"{cnt:,} | {mx:,} | `{o}` |")
            if i <= args.inline:
                inline_sites.append(site)
        rest = sum(v[0] for _, v in ranked[args.top :])
        if rest:
            print(f"| | {rest:,} | {100 * rest / max(nb, 1):.1f} | 100.0 | "
                  f"{sum(v[1] for _, v in ranked[args.top:]):,} | | ({len(ranked) - args.top} more owners) |")
        print()
        if freed:
            fg = defaultdict(lambda: [0, 0])
            for r in freed:
                o, _ = owner_of(r)
                fg[o][0] += recs_size[r]
                fg[o][1] += 1
            print(f"Freed in the window, top {min(10, len(fg))} owners:")
            print()
            for o, (byt, cnt) in sorted(fg.items(), key=lambda kv: -kv[1][0])[:10]:
                print(f"- {byt:,} B in {cnt} block(s): `{o}`")
            print()

    if failed:
        print("## Failed allocations (null returned)")
        print()
        fg = defaultdict(lambda: [0, 0, 0])
        for bt, cyc, size, frames in failed:
            o = owner(frames)[0]
            fg[o][0] += 1
            fg[o][1] = max(fg[o][1], size)
            fg[o][2] = bt
        for o, (cnt, mx, bt) in sorted(fg.items(), key=lambda kv: -kv[1][0])[:10]:
            print(f"- {cnt} failed, largest {mx:,} B (boot {bt}): `{o}`")
        print()

    if inline_sites:
        tool = shutil.which("addr2line") or "/opt/homebrew/opt/binutils/bin/addr2line"
        print("## Inlined call sites of the top owners (addr2line -i)")
        print()
        seen = set()
        for site in inline_sites:
            if site in seen:
                continue
            seen.add(site)
            out = subprocess.run(
                [tool, "-pfiaC", "-e", args.elf, f"{site - 1:#x}"],
                capture_output=True, text=True,
            ).stdout.strip()
            print("```")
            print(re.sub(r"::h[0-9a-f]{16}", "", out))
            print("```")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())

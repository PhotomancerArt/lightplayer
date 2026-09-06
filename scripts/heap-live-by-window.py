#!/usr/bin/env python3
"""Attribute a profile's live heap set, at end of trace, by birth window and call site.

`lp-cli profile --collect alloc` writes `heap-trace.jsonl` (every alloc/realloc/
dealloc event, plus perf-event markers) and `meta.json` (the guest's symbol
table) into a profile directory. `report.txt`'s "Live Allocations" section
already shows the live set by call site; this script adds the axis that report
does not carry — which perf-event *window* each live block was born in (the
window open when its allocation event was recorded) — so a birth window like
"frame" or "project-load" can be read off directly instead of re-derived by
hand from the trace.

Usage:
    heap-live-by-window.py <profile dir> [--min-bytes N]

Prints, in order:
  1. the perf-event markers, with live bytes and block count at each;
  2. live bytes and block count at end of trace, by birth window;
  3. every live block >= --min-bytes (default 1000), with its birth window and
     an attributed call site (demangled with `rustfilt` when available);
  4. live bytes at end of trace by call site, for blocks born in the "frame"
     window only (all sizes) — the per-lamp residents live here.

Requires `rustfilt` on PATH to demangle Rust symbol names; without it, this
prints mangled names and says so on stderr once.
"""

from __future__ import annotations

import argparse
import bisect
import json
import shutil
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

# Frames that are the allocator's own machinery, not the caller that decided to
# allocate. Skipped when picking the one frame that attributes a live block to
# a call site.
ALLOCATOR_FRAMES = (
    "TrackingAllocator",
    "raw_vec",
    "__rust_alloc",
    "alloc::alloc",
    "finish_grow",
    "do_reserve",
    "grow_one",
    "try_allocate_in",
    "allocate_in",
)

# How many stack frames to look at per live block: enough to walk past
# allocator machinery and print a short call chain, not so many the chain
# printout gets unwieldy.
FRAMES_PER_BLOCK = 7


def load_symbols(profile_dir: Path) -> tuple[list[int], list[dict]]:
    meta = json.load(open(profile_dir / "meta.json"))
    syms = sorted(meta["symbols"], key=lambda s: s["addr"])
    addrs = [s["addr"] for s in syms]
    return addrs, syms


def make_symbolizer(addrs: list[int], syms: list[dict]):
    def sym(pc: int) -> str:
        i = bisect.bisect_right(addrs, pc) - 1
        if i < 0:
            return f"?{pc}"
        s = syms[i]
        return s["name"] if pc < s["addr"] + s["size"] else f"?{pc}"

    return sym


def demangle(names: list[str]) -> dict[str, str]:
    """Map mangled names to demangled ones via `rustfilt`. Falls back to the
    identity mapping (with a stderr note, once) when `rustfilt` is absent."""
    if shutil.which("rustfilt") is None:
        print(
            "heap-live-by-window: rustfilt not found on PATH; printing mangled symbol names",
            file=sys.stderr,
        )
        return {n: n for n in names}
    out = subprocess.run(
        ["rustfilt"], input="\n".join(names), capture_output=True, text=True
    )
    if out.returncode != 0:
        print(
            f"heap-live-by-window: rustfilt exited {out.returncode}; printing mangled symbol names",
            file=sys.stderr,
        )
        return {n: n for n in names}
    return dict(zip(names, out.stdout.splitlines()))


def load_live_set(profile_dir: Path):
    """Replay heap-trace.jsonl and return (live, markers).

    live: {ptr: (size, birth_window, frames)} for every block live at end of
    trace. birth_window is the perf-event window open when the block's
    allocation (or, for a realloc, its most recent reallocation) was recorded.
    markers: [(name, kind, ic, live_bytes_then, block_count_then), ...].
    """
    live: dict[int, tuple[int, str, tuple[int, ...]]] = {}
    window = "pre"
    markers = []
    with open(profile_dir / "heap-trace.jsonl") as f:
        for line in f:
            r = json.loads(line)
            t = r["t"]
            if t == "P":
                if r["kind"] == "B":
                    window = r["name"]
                elif r["kind"] == "E":
                    window = f"after-{r['name']}"
                markers.append(
                    (
                        r["name"],
                        r["kind"],
                        r["ic"],
                        sum(v[0] for v in live.values()),
                        len(live),
                    )
                )
            elif t == "A":
                live[r["ptr"]] = (r["sz"], window, tuple(r.get("frames", [])))
            elif t == "R":
                live.pop(r["old_ptr"], None)
                live[r["ptr"]] = (r["sz"], window, tuple(r.get("frames", [])))
            elif t == "D":
                live.pop(r["ptr"], None)
    return live, markers


def attribute_site(frames: tuple[int, ...], sym, demangled: dict[str, str]):
    """Return (site, chain) for one live block's stack: `site` is the first
    frame that is not allocator machinery (falls back to the innermost frame);
    `chain` is a short human-readable call chain for the frames considered."""
    chain_names = [demangled.get(sym(pc), sym(pc)) for pc in frames[:FRAMES_PER_BLOCK]]
    site = next(
        (n for n in chain_names if not any(s in n for s in ALLOCATOR_FRAMES)),
        chain_names[0] if chain_names else "?",
    )
    chain = " <- ".join("::".join(n.split("::")[-2:]) for n in chain_names)
    return site, chain


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Attribute a profile's live heap set, at end of trace, "
        "by birth window and call site.",
    )
    parser.add_argument("profile_dir", type=Path, help="a lp-cli profile output directory")
    parser.add_argument(
        "--min-bytes",
        type=int,
        default=1000,
        help="only list individual live blocks at or above this size (default: 1000)",
    )
    args = parser.parse_args()

    addrs, syms = load_symbols(args.profile_dir)
    sym = make_symbolizer(addrs, syms)
    live, markers = load_live_set(args.profile_dir)

    print("markers (live bytes, blocks):")
    for name, kind, ic, live_bytes, blocks in markers:
        print(f"  {name:14} {kind}  ic={ic:>12,}  live={live_bytes:>9,}  blocks={blocks}")

    total = sum(v[0] for v in live.values())
    print(f"\nlive at end: {total:,} B in {len(live)} blocks")

    by_window: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    for sz, w, _ in live.values():
        by_window[w][0] += sz
        by_window[w][1] += 1
    for w, (b, n) in sorted(by_window.items(), key=lambda kv: -kv[1][0]):
        print(f"  {w:22} {b:>9,} B  {n} blocks")

    names = sorted({sym(pc) for _, _, frames in live.values() for pc in frames[:FRAMES_PER_BLOCK]})
    demangled = demangle(names)

    print(f"\nlive blocks >= {args.min_bytes} B at end, by birth window:")
    big_blocks = sorted(
        (v for v in live.values() if v[0] >= args.min_bytes), key=lambda r: -r[0]
    )
    for sz, w, frames in big_blocks:
        site, chain = attribute_site(frames, sym, demangled)
        print(f"  {sz:>8,}  {w:18}  {site[:90]}\n            {chain[:200]}")

    print("\nframe-window live bytes by site (all sizes):")
    agg: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    for sz, w, frames in live.values():
        if w == "frame":
            site, _ = attribute_site(frames, sym, demangled)
            agg[site][0] += sz
            agg[site][1] += 1
    for site, (b, n) in sorted(agg.items(), key=lambda kv: -kv[1][0])[:30]:
        print(f"  {b:>8,} B  {n:>4} blocks  {site[:110]}")

    return 0


if __name__ == "__main__":
    sys.exit(main())

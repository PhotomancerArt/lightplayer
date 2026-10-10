#!/usr/bin/env python3
"""RESEARCH (research/ram-e03): live heap bytes over an alloc trace, split
LP SRAM / HP SRAM — the peak of each, when, and who allocated in LP SRAM.

    python3 scripts/ram/e03-trace-peaks.py <trace> [--after 'log text']

Regions are told apart by address only: LP SRAM is 0x5000_0000..0x5000_4000,
everything else is HP SRAM. Sizes are the requested sizes the hooks saw (the
allocator rounds each up a little). `--after` starts the peak search at the
first `L` line containing that text (the live set is still counted from the
start). Symbolize the LP backtraces with `scripts/ram/alloc-trace-report.py`
or `rust-addr2line`.
"""
import argparse
import gzip

ap = argparse.ArgumentParser()
ap.add_argument("trace")
ap.add_argument("--after", default=None)
ap.add_argument(
    "--skip-range",
    action="append",
    default=[],
    help="START:LEN (hex) of a function whose allocations are left out — e.g. "
    "`largest_free_block`'s trial blocks, freed at once (`rust-nm -S -C p2.elf`)",
)
args = ap.parse_args()
skips = [(int(a, 16), int(a, 16) + int(b, 16)) for a, b in (r.split(":") for r in args.skip_range)]


def skipped(frames):
    for word in frames.split(","):
        if word:
            pc = int(word, 16)
            if any(lo <= pc < hi for lo, hi in skips):
                return True
    return False


def region(addr):
    return "lp" if 0x5000_0000 <= addr < 0x5000_4000 else "hp"


opener = gzip.open if args.trace.endswith(".gz") else open
live = {}
total = {"hp": 0, "lp": 0}
peak = {"hp": (0, 0, ""), "lp": (0, 0, ""), "all": (0, 0, "")}
armed = args.after is None
last_log = ""
lp_sites = {}
lp_allocs = 0
with opener(args.trace, "rt") as f:
    for line in f:
        if line.startswith("L "):
            last_log = line.split(" ", 2)[2].strip()
            if not armed and args.after in line:
                armed = True
            continue
        if line.startswith("M ") and "@reboot" in line:
            live.clear()
            total = {"hp": 0, "lp": 0}
            continue
        parts = line.split()
        if not parts or parts[0] not in ("A", "F"):
            continue
        addr = int(parts[2], 16)
        if parts[0] == "A":
            if addr == 0 or (skips and skipped(parts[5] if len(parts) > 5 else "")):
                continue
            size = int(parts[3])
            reg = region(addr)
            live[addr] = (size, reg)
            total[reg] += size
            if reg == "lp":
                lp_allocs += 1
                frames = parts[5] if len(parts) > 5 else ""
                lp_sites[frames] = lp_sites.get(frames, 0) + size
        elif addr in live:
            size, reg = live.pop(addr)
            total[reg] -= size
        if armed:
            cyc = int(parts[1])
            for k in ("hp", "lp"):
                if total[k] > peak[k][0]:
                    peak[k] = (total[k], cyc, last_log)
            s = total["hp"] + total["lp"]
            if s > peak["all"][0]:
                peak["all"] = (s, cyc, last_log)

print(f"end live: hp {total['hp']:,} B, lp {total['lp']:,} B")
for k, (v, c, log) in peak.items():
    print(f"peak {k}: {v:,} B at cycle {c:,} ({c / 160e6:.3f} s at 160 MHz) — last log: {log[:140]}")
print(f"lp: {lp_allocs:,} allocations ever, {len(lp_sites)} distinct backtraces; top 10 by bytes:")
for frames, size in sorted(lp_sites.items(), key=lambda kv: -kv[1])[:10]:
    print(f"  {size:,} B  {frames}")

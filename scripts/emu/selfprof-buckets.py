#!/usr/bin/env python3
"""Symbolize a `--features selfprof` pc histogram and bucket the host time.

    scripts/emu/selfprof-buckets.py <pcs-file> --bin <binary> [--dsym <dSYM>]
                                    [--raw] [--top 25]

The histogram comes from `LP_EMU_SELFPROF=<path>` on a machine binary built
with `--features selfprof` (see `lp-emu/esp/lp-emu-esp32v3/src/selfprof.rs`).
Each line is `<runtime address> <static address> <samples>`.

`/usr/bin/sample` cannot answer this question: at `opt-level = 3` fetch,
decode, the window check, dispatch and execute all inline into one symbol.
`atos --inlineFrames` recovers the inlined frame chain, and the **innermost**
frame that matches a rule below is what the sample is attributed to — so a
decode that was inlined into `run_slice` is counted as decode, not as "the
run loop".

The buckets are the ones M7's roadmap names for the classic, plus two the
classic makes visible that the C6's list did not: the guest's own RAM
accesses, and the mask ROM's SPI-flash routines.

⚠️ **A sampled share is an attribution, not an ablation.** It says where the
seconds went, not what removing that work would buy — an inlined bucket can
be cheap to *attribute* and impossible to *remove*. Every rung this feeds
still has to be measured as a same-window A/B.

Needs `atos` and (for inline frames) a dSYM built with `dsymutil`; the script
builds one beside the binary if `--dsym` is not given and none is there.
"""

import argparse
import collections
import os
import re
import subprocess
import sys

# How a frame is bucketed.
#
# ⚠️ **Match the OWNING crate, not "does the symbol mention this crate".**
# Rust's v0 mangling embeds generic arguments, so nearly every `lp_xt_emu`
# symbol in this binary carries `17lp_emu_esp_common3bus6SocBus` inside it —
# the hart is generic over its bus. A substring rule on the crate name puts
# the whole interpreter in the bus's bucket, which is how this script read
# 41 % "guest RAM access" before the rule below existed. The owning crate is
# the FIRST `Cs<hash>_<len><name>` in the symbol, which is the path root.
#
# The file name is not a substitute either: at `opt-level = 3` a
# `core::slice::get` inlined into the bus's RAM path prints as `(mod.rs:576)`,
# and so does `lp_xt_emu::mach::mod.rs`.
#
# Frames from `core`/`alloc`, and LLVM's outlined functions, are TRANSPARENT:
# inlined helpers with no attribution value of their own, so the walk looks one
# frame further out. If a whole chain is transparent the sample lands in
# `other`, which is the honest answer.

CRATE_RE = re.compile(r"Cs[A-Za-z0-9]+_(\d+)([A-Za-z0-9_]+)")
FILE_RE = re.compile(r"\(([A-Za-z0-9_.]+):\d+\)\s*$")

# (bucket, owning crate or None for any, regex over "symbol filename" or None).
# ORDER IS THE PRIORITY: the first matching row wins. Fetch is its own bucket
# because it is the one the C6's ladder found a twenty-line cache could remove
# outright.
RULES = [
    ("decode", "lp_xt_inst", None),
    ("window machinery", "lp_xt_emu", r"6window|4trap|window\.rs|trap\.rs"),
    ("interrupt sampling", "lp_xt_emu", r"9interrupt|interrupt\.rs"),
    ("interrupt sampling", None, r"resample_external|pending_cpu_interrupt|intmatrix\.rs"),
    ("guest fetch", "lp_emu_esp_common", r"fetch_bytes|fetch_instruction|fetch_region_index"),
    ("MMIO", "lp_emu_esp_common", r"read_mmio|write_mmio|mmio_index"),
    ("MMIO", None, r"\b(uart|rmt|timg|dport|gpio|rtc_cntl|spi|efuse|flash|cache|regfile)\.rs"),
    ("guest RAM access", "lp_emu_core", r"5arena|GuestArena|arena\.rs"),
    ("scheduler + interleave", "lp_emu_core", None),
    ("scheduler + interleave", "lp_emu_esp32v3", None),
    ("scheduler + interleave", "lp_emu_esp_common", r"\b(sched|pins|host|trace)\.rs"),
    ("guest RAM access", "lp_emu_esp_common", None),
    ("execute", "lp_xt_emu", None),
]

# Frames that say nothing about which concern a sample belongs to.
TRANSPARENT = re.compile(r"^OUTLINED_FUNCTION|^0x|^_ZN")
TRANSPARENT_CRATES = {"core", "alloc", "std", "compiler_builtins"}

# Files that name a concern even when every frame in the chain is transparent.
# `prefilter.rs`/`twoway.rs`/`memchr.rs` are `memchr`, which is what
# `--exit-on`'s substring search runs on every byte the guest prints — a real
# cost of the MEASUREMENT rather than of the machine, and worth seeing as its
# own row instead of hiding inside `other`.
FALLBACK_FILES = re.compile(r"prefilter\.rs|twoway\.rs|memchr\.rs|host\.rs")


def owning_crate(symbol):
    """The crate a mangled symbol is DEFINED in — the first `Cs…_<len><name>`.

    Later ones are generic arguments, which is the whole trap this exists to
    avoid.
    """
    m = CRATE_RE.search(symbol)
    if not m:
        return None
    return m.group(2)[: int(m.group(1))]


def read_pcs(path):
    rows = []
    with open(path) as f:
        for line in f:
            if line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) == 3:
                rows.append((parts[1], int(parts[2])))
    return rows


def ensure_dsym(binary, dsym):
    if dsym:
        return dsym
    guess = binary + ".dSYM"
    if not os.path.isdir(guess):
        print(f"selfprof-buckets: dsymutil {binary}", file=sys.stderr)
        subprocess.run(["dsymutil", binary, "-o", guess], check=True)
    return guess


def dwarf_binary(dsym, binary):
    d = os.path.join(dsym, "Contents", "Resources", "DWARF")
    if os.path.isdir(d):
        names = os.listdir(d)
        if names:
            return os.path.join(d, names[0])
    return binary


def symbolize(addresses, obj):
    """One atos call. Returns a list of frame-chains, one per address."""
    proc = subprocess.run(
        ["atos", "-o", obj, "-l", "0x100000000", "--inlineFrames"],
        input="\n".join(addresses) + "\n",
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.exit(f"selfprof-buckets: atos failed: {proc.stderr.strip()}")
    # With `--inlineFrames` atos answers EVERY address with its frame chain
    # followed by one blank line — including an address it cannot resolve,
    # which it answers with the address itself. So the answer is one group per
    # address, empty groups included: an empty group would mean atos printed
    # nothing at all, and dropping it here is what silently misaligns the
    # chains with the counts.
    body = proc.stdout.rstrip("\n")
    if not body:
        return []
    return [
        [line.strip() for line in group.split("\n") if line.strip()]
        for group in body.split("\n\n")
    ]


def bucket_of(chain):
    # Innermost first, skipping frames that carry no attribution.
    for frame in chain:
        symbol = frame.split(" (in ")[0]
        if TRANSPARENT.search(symbol):
            continue
        crate = owning_crate(symbol)
        if crate in TRANSPARENT_CRATES:
            continue
        m = FILE_RE.search(frame)
        where = symbol + " " + (m.group(1) if m else "")
        for name, want_crate, pattern in RULES:
            if want_crate is not None and crate != want_crate:
                continue
            if pattern is not None and not re.search(pattern, where):
                continue
            return name, frame
    # A chain that is transparent all the way out still has a FILE, and for
    # the two that matter here — `memchr`'s prefilter behind `--exit-on`, and
    # the host sinks — that file is the whole answer.
    for frame in chain:
        m = FILE_RE.search(frame)
        if m and FALLBACK_FILES.search(m.group(1)):
            return "host stream + --exit-on", frame
    # Nothing in the chain named a concern: report the innermost frame so the
    # `other` rows say what they were rather than only how many.
    return "other", chain[0] if chain else "?"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pcs")
    ap.add_argument("--bin", required=True)
    ap.add_argument("--dsym")
    ap.add_argument("--top", type=int, default=25)
    ap.add_argument("--raw", action="store_true", help="print every frame chain")
    args = ap.parse_args()

    rows = read_pcs(args.pcs)
    total = sum(n for _, n in rows)
    obj = dwarf_binary(ensure_dsym(args.bin, args.dsym), args.bin)
    chains = symbolize([a for a, _ in rows], obj)
    if len(chains) != len(rows):
        sys.exit(
            f"selfprof-buckets: atos answered {len(chains)} of {len(rows)} "
            "addresses — the chain separator assumption is wrong for this atos"
        )

    buckets = collections.Counter()
    sites = collections.Counter()
    for (_, n), chain in zip(rows, chains):
        name, frame = bucket_of(chain)
        buckets[name] += n
        sites[(name, frame)] += n
        if args.raw:
            print(f"{n:6d}  [{name}]  " + " <- ".join(chain))

    print(f"\nsamples {total:,} over {len(rows):,} distinct pcs, "
          f"from {args.pcs}\n")
    print("| bucket | samples | share of host time |")
    print("|---|---:|---:|")
    order = []
    for name, _, _ in RULES:
        if name not in order:
            order.append(name)
    order.append("host stream + --exit-on")
    order.append("other")
    for name in order:
        n = buckets.get(name, 0)
        if n:
            print(f"| {name} | {n:,} | {100 * n / total:.1f}% |")
    print(f"| **total** | **{total:,}** | **100.0%** |")

    print("\n### The hottest attributed frames\n")
    print("| bucket | frame | samples | share |")
    print("|---|---|---:|---:|")
    for (name, frame), n in sites.most_common(args.top):
        frame = frame.replace("|", "\\|")
        print(f"| {name} | `{frame}` | {n:,} | {100 * n / total:.1f}% |")


if __name__ == "__main__":
    main()

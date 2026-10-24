#!/usr/bin/env python3
"""The ESP32-S3's IRAM hot set: which flash functions the linker puts in RAM.

The S3 runs most of its non-shader frame time in the engine's per-lane graph
walk, fetched from flash at ~12.5 cycles an instruction because that code
overflows the 32 KB instruction cache (fps spike, 2026-10-23). Linking the
most-sampled ~40 KB of it into `.rwtext` (internal SRAM, no cache) halves it.
Which functions is a measurement, not a list anyone maintains by hand, and
this script is the only thing that writes it. Three steps:

    fold   silicon PC samples  -> lp-fw/fw-esp32s3/hot_iram/profile.tsv
    gen    profile.tsv         -> lp-fw/fw-esp32s3/rwtext_hook.x
    check  a linked image      -> is the hook still applied, and within budget?

`fold` reads the `[pcs]` lines the `pc-sampler` image prints (core 1 reading
core 0's PC from the debug assistant; `lp-fw/fw-esp32s3/src/pc_sampler.rs`)
and the ELF that printed them. Each sampled PC in flash text becomes a
function's sample; the profile keeps the function's demangled name (stable
across builds), its size in that build, and a linker pattern for it.

`gen` is greedy by samples per byte under a byte budget, over the profile
alone: the same profile always writes the same hook, whatever the tree
looks like, so `gen` never churns on an unrelated change.

`check` reads a linked image (plain, or a split image's `p2.elf`) and fails
when a pattern places a function anywhere but IRAM (the hook is not being
applied), when the hot set's bytes exceed the arena's reservation
(`HOT_IRAM_BYTES` in `lp-fw/fw-esp32s3/src/main.rs`), or when functions the
hook names have vanished from the image (renamed, inlined away) carrying more
than `--max-stale` of the hot set's samples. Then the profile is stale: rename
its entry if the function was renamed, `gen` again to backfill from the
profile, or take a new profile on FS3 (lp-fw/fw-esp32s3/hot_iram/README.md).

Linker patterns are the function's section names, `.literal.<sym>` and
`.text.<sym>`, with the parts of a Rust v0 symbol that change from build to
build globbed: the crate disambiguator (`Cs<base62>_`) and back-references
(`B<base62>_`, whose offsets move when a disambiguator's length does), and an
LTO `.llvm.<n>` suffix. GNU ld matches section names with the same glob as
Python's `fnmatch`.

    scripts/s3-hot-iram.py fold <elf> <console>... [--out profile.tsv] [--note TEXT]
    scripts/s3-hot-iram.py gen [--profile profile.tsv] [--budget 40960] [--out rwtext_hook.x]
    scripts/s3-hot-iram.py check <elf> [--hook rwtext_hook.x] [--profile profile.tsv]
                                 [--budget-bytes N] [--max-stale 0.10]
"""

from __future__ import annotations

import argparse
import bisect
import collections
import fnmatch
import glob
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CRATE = REPO / "lp-fw" / "fw-esp32s3"
PROFILE = CRATE / "hot_iram" / "profile.tsv"
HOOK = CRATE / "rwtext_hook.x"
MAIN_RS = CRATE / "src" / "main.rs"

# Flash-mapped code on the S3 (`lp_bootctl::s3_mmu`): the core's pages from
# entry 0, and a split image's engine at entry 0x100. Entries 0x80..0x100 are
# the PSRAM window, where JIT code runs.
FLASH_TEXT = ((0x4200_0000, 0x4280_0000), (0x4300_0000, 0x4400_0000))
PSRAM_TEXT = (0x4280_0000, 0x4300_0000)
# SRAM0 + SRAM1's instruction bus (esp-hal `ld/esp32s3/memory.x`).
IRAM = (0x4037_0000, 0x403E_0000)

_PCS = re.compile(r"\[pcs\] \d+\.\d+ ((?:[0-9a-f]{8} ?)+)")
_LLVM_SUFFIX = re.compile(r"\.llvm\.\d+$")
# v0: a crate root `C` + disambiguator `s<base62>_` + `<len><name>`, and a
# back-reference `B<base62>_`. Globbed so the pattern survives a rebuild.
_V0_CRATE = re.compile(r"Cs[0-9A-Za-z]*_(?=\d)")
_V0_BACKREF = re.compile(r"B[0-9A-Za-z]*_")


def tool(name: str) -> str:
    found = shutil.which(name)
    if found:
        return found
    pattern = os.path.expanduser(
        f"~/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin/{name}"
    )
    hits = sorted(glob.glob(pattern))
    if hits:
        return hits[-1]
    sys.exit(f"s3-hot-iram: {name} not found (PATH, or espup's toolchain)")


def nm_functions(elf: str) -> list[tuple[int, int, str, str]]:
    """`(address, size, mangled, demangled)` for every function symbol."""
    nm = tool("xtensa-esp32s3-elf-nm")
    raw = subprocess.run(
        [nm, "--print-size", "-n", elf], check=True, capture_output=True, text=True
    ).stdout.splitlines()
    dem = subprocess.run(
        [nm, "--print-size", "-n", "-C", elf], check=True, capture_output=True, text=True
    ).stdout.splitlines()
    out = []
    for r, d in zip(raw, dem):
        pr, pd = r.split(" ", 3), d.split(" ", 3)
        if len(pr) == 4 and pr[2] in "tTwW":
            out.append((int(pr[0], 16), int(pr[1], 16), pr[3], stable_name(pd[3])))
    return out


def stable_name(demangled: str) -> str:
    return _LLVM_SUFFIX.sub("", demangled)


def pattern_of(mangled: str) -> str:
    sym = _LLVM_SUFFIX.sub("*", mangled)
    if sym.startswith("_R"):
        sym = _V0_BACKREF.sub("B*_", _V0_CRATE.sub("Cs*_", sym))
    return sym


def in_range(addr: int, lo_hi: tuple[int, int]) -> bool:
    return lo_hi[0] <= addr < lo_hi[1]


def in_flash_text(addr: int) -> bool:
    return any(in_range(addr, r) for r in FLASH_TEXT)


# ---------------------------------------------------------------- fold


def fold(args: argparse.Namespace) -> int:
    funcs = [f for f in nm_functions(args.elf) if f[1] > 0]
    addrs = [f[0] for f in funcs]
    hits: collections.Counter[int] = collections.Counter()
    total = jit = flash = iram = other = 0
    for console in args.consoles:
        for line in open(console, errors="replace"):
            m = _PCS.search(line)
            if not m:
                continue
            for word in m.group(1).split():
                pc = int(word, 16)
                total += 1
                if in_range(pc, PSRAM_TEXT):
                    jit += 1
                    continue
                if in_range(pc, IRAM):
                    iram += 1
                    continue
                i = bisect.bisect_right(addrs, pc) - 1
                if in_flash_text(pc) and i >= 0 and pc < funcs[i][0] + funcs[i][1]:
                    flash += 1
                    hits[i] += 1
                else:
                    other += 1
    if not total:
        sys.exit("s3-hot-iram fold: no [pcs] samples in the consoles")
    # One row per stable name: duplicates of one function (two copies the
    # linker kept) add their samples, and carry every pattern.
    rows: dict[str, list] = {}
    for i, n in hits.items():
        _, size, mangled, name = funcs[i]
        row = rows.setdefault(name, [0, 0, set()])
        row[0] += n
        row[1] = max(row[1], size)
        row[2].add(pattern_of(mangled))
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w") as f:
        f.write("# The S3's IRAM hot set's profile. Written by scripts/s3-hot-iram.py fold; read by gen.\n")
        f.write(f"# image: {Path(args.elf).name}; consoles: {', '.join(Path(c).name for c in args.consoles)}\n")
        if args.note:
            f.write(f"# {args.note}\n")
        f.write(
            f"# samples {total}: flash text {flash}, JIT (PSRAM) {jit}, IRAM {iram}, elsewhere {other}\n"
        )
        f.write("# samples\tbytes\tfunction\tpatterns (space-separated)\n")
        for name, (n, size, pats) in sorted(rows.items(), key=lambda kv: (-kv[1][0], kv[0])):
            f.write(f"{n}\t{size}\t{name}\t{' '.join(sorted(pats))}\n")
    print(
        f"fold: {total} samples, {flash} in flash text ({len(rows)} functions), "
        f"{jit} JIT, {iram} IRAM, {other} elsewhere -> {out}"
    )
    return 0


# ---------------------------------------------------------------- profile


class Entry:
    def __init__(self, samples: int, size: int, name: str, patterns: list[str]):
        self.samples, self.size, self.name, self.patterns = samples, size, name, patterns


def read_profile(path: Path) -> list[Entry]:
    entries = []
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        samples, size, name, pats = line.split("\t")
        entries.append(Entry(int(samples), int(size), name, pats.split(" ")))
    return entries


def choose(entries: list[Entry], budget: int) -> list[Entry]:
    # Samples per byte, a function no smaller than 16 B so a tiny one does not
    # win on a rounding error; ties by name, so the order is the profile's.
    ranked = sorted(entries, key=lambda e: (-e.samples / max(e.size, 16), e.name))
    used, chosen = 0, []
    for e in ranked:
        if used + e.size > budget:
            continue
        used += e.size
        chosen.append(e)
    return chosen


# ---------------------------------------------------------------- gen


def gen(args: argparse.Namespace) -> int:
    entries = read_profile(Path(args.profile))
    chosen = choose(entries, args.budget)
    flash = sum(e.samples for e in entries)
    got = sum(e.samples for e in chosen)
    used = sum(e.size for e in chosen)
    with open(args.out, "w") as f:
        f.write(
            "/* The S3's IRAM hot set: flash functions linked into `.rwtext`, most-sampled first.\n"
            "   GENERATED by `just s3-hot-iram-gen` (scripts/s3-hot-iram.py gen) from\n"
            "   lp-fw/fw-esp32s3/hot_iram/profile.tsv; do not edit by hand.\n"
            f"   {len(chosen)} functions, {used} B at capture, {100 * got / max(flash, 1):.1f} % of the\n"
            "   profile's flash-text samples. esp-hal INCLUDEs this file inside `.rwtext` when\n"
            "   ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK is set (lp-fw/fw-esp32s3/.cargo/config.toml). */\n"
        )
        for e in chosen:
            f.write(f"  /* {e.samples} {e.size} {e.name[:150].replace('*/', '* /')} */\n")
            for p in e.patterns:
                # LLVM puts a function it thinks cold in `.text.unlikely.<sym>`;
                # sampled hot on silicon all the same.
                f.write(f"  *(.literal.{p} .text.{p} .literal.unlikely.{p} .text.unlikely.{p})\n")
    print(
        f"gen: {len(chosen)} of {len(entries)} functions, {used} B (budget {args.budget}), "
        f"{100 * got / max(flash, 1):.1f} % of flash-text samples -> {args.out}"
    )
    return 0


# ---------------------------------------------------------------- check

_HOOK_RULE = re.compile(r"\*\(\.literal\.(\S+) \.text\.(\S+)[ )]")
_HOOK_NOTE = re.compile(r"/\* (\d+) (\d+) ")


def hook_rules(path: Path) -> list[tuple[int, str]]:
    """`(samples, pattern)` per rule, the samples from the comment above it."""
    rules, samples = [], 0
    for line in path.read_text().splitlines():
        if note := _HOOK_NOTE.search(line):
            samples = int(note.group(1))
        if rule := _HOOK_RULE.search(line):
            rules.append((samples, rule.group(2)))
    return rules


def reserved_bytes() -> int | None:
    m = re.search(r"const HOT_IRAM_BYTES: usize = (\d+) \* 1024;", MAIN_RS.read_text())
    return int(m.group(1)) * 1024 if m else None


def check(args: argparse.Namespace) -> int:
    rules = hook_rules(Path(args.hook))
    if not rules:
        sys.exit(f"s3-hot-iram check: no rules in {args.hook}")
    budget = args.budget_bytes or reserved_bytes()
    funcs = nm_functions(args.elf)
    failures = []
    placed_bytes = 0
    in_iram = set()
    stale_samples = 0
    stale = []
    in_flash = []
    for samples, pat in rules:
        matched = [f for f in funcs if fnmatch.fnmatchcase(f[2], pat)]
        if not matched:
            stale_samples += samples
            stale.append((samples, pat))
            continue
        placed = False
        for addr, size, mangled, name in matched:
            if in_range(addr, IRAM):
                placed = True
                if mangled not in in_iram:
                    in_iram.add(mangled)
                    placed_bytes += size
            else:
                in_flash.append((addr, name))
        if not placed:
            # The symbol is there but its section is not the one the pattern
            # names (an alias: `calloc_internal` is `calloc`'s second name,
            # in `.text.calloc`). Counted with the stale ones.
            stale_samples += samples
            stale.append((samples, pat))
    total = sum(s for s, _ in rules)
    share = stale_samples / max(total, 1)
    print(
        f"check: {len(rules)} rules; {len(in_iram)} functions in IRAM, {placed_bytes} B of code "
        f"(literal pools beside them); reservation {budget} B; stale {len(stale)} rules "
        f"= {100 * share:.1f} % of the hot set's samples"
    )
    for s, pat in sorted(stale, reverse=True)[:12]:
        print(f"  stale ({s} samples): {pat}")
    for addr, name in in_flash[:8]:
        print(f"  in flash, not IRAM: {addr:#010x} {name[:120]}")
    if not in_iram:
        failures.append(
            "no hot function is in IRAM: the hook is not applied "
            "(ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK in lp-fw/fw-esp32s3/.cargo/config.toml?)"
        )
    # Literal pools ride beside the code and carry no symbol: 5.7 % of the
    # code on the first hot set (2,332 B over 40,964 B, `.rwtext` before and
    # after, 2026-10-24); 7 % is the allowance.
    with_pools = placed_bytes * 107 // 100
    if budget is not None and with_pools > budget:
        failures.append(
            f"hot set ~{with_pools} B with its literal pools exceeds the arena's reservation {budget} B "
            "(raise HOT_IRAM_BYTES in lp-fw/fw-esp32s3/src/main.rs, or gen with a smaller budget)"
        )
    if share > args.max_stale:
        failures.append(
            f"{100 * share:.1f} % of the hot set's samples name functions this image no longer has "
            f"(limit {100 * args.max_stale:.0f} %): `just s3-hot-iram-gen` to backfill from the profile, "
            "or re-profile on FS3 (lp-fw/fw-esp32s3/hot_iram/README.md)"
        )
    for f in failures:
        print(f"FAIL: {f}")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fold")
    f.add_argument("elf")
    f.add_argument("consoles", nargs="+")
    f.add_argument("--out", default=str(PROFILE))
    f.add_argument("--note")
    g = sub.add_parser("gen")
    g.add_argument("--profile", default=str(PROFILE))
    g.add_argument("--budget", type=int, default=40960)
    g.add_argument("--out", default=str(HOOK))
    c = sub.add_parser("check")
    c.add_argument("elf")
    c.add_argument("--hook", default=str(HOOK))
    c.add_argument("--profile", default=str(PROFILE))
    c.add_argument("--budget-bytes", type=int)
    c.add_argument("--max-stale", type=float, default=0.10)
    args = ap.parse_args()
    return {"fold": fold, "gen": gen, "check": check}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Read a classic-ESP32 `--features bench` dump and say what the guest did.

    scripts/emu/bench-esp32v3-counts.py <counts-file> --elf <firmware.elf>
                                        [--top 15] [--markdown]

The dump comes from `LP_EMU_V3_BENCHPROF=<path> lp-emu-esp32v3` built with
`--features bench` (see `lp-emu/esp/lp-emu-esp32v3/src/benchdump.rs`). It
holds three facts and no opinions: instructions and windows per core, MMIO
accesses per register address, and one line per retired instruction pc.

This turns those into the four numbers M7's roadmap asks of the classic:

  1. **MMIO share and its top sites** — straight out of the dump, against the
     RAM load/store totals beside it.
  2. **Window-overflow / underflow rate** — fetches landing exactly on
     `_WindowOverflow{4,8,12}` / `_WindowUnderflow{4,8,12}`, whose addresses
     come from the image's own symbol table. Every entry into a window
     handler fetches its first instruction, and nothing else in the image
     branches there, so the count of fetches at those six addresses IS the
     count of window exceptions taken.
  3. **LOOP hotness** — the share of retired instructions inside a
     zero-overhead loop body, where the bodies come from the image's own
     disassembly: every `loop`/`loopnez`/`loopgtz` names its end address, so
     `(instruction after the loop, end)` is the body.
  4. **The cross-core switch rate** — windows given per core against
     instructions retired per core.

⚠️ **The static scan cannot see the JIT.** The shader compiles to Xtensa
machine code in RAM at run time (`lpvm-native`), so a pc in the JIT region is
in no ELF section and inside no disassembled loop. That share is reported
separately, as `outside the image`, rather than being silently counted as
"not in a loop" — a LOOP hotness figure that quietly folded the JIT in would
be wrong in the one region the render loop spends most of its time.

Needs `xtensa-esp32-elf-nm` and `xtensa-esp32-elf-objdump` on PATH
(`just _xt-gcc-dir xtensa-esp32-elf-gcc` is the lookup the build recipes use).
Pure stdlib otherwise.
"""

import argparse
import re
import shutil
import subprocess
import sys

WINDOW_SYMBOLS = [
    "_WindowOverflow4",
    "_WindowOverflow8",
    "_WindowOverflow12",
    "_WindowUnderflow4",
    "_WindowUnderflow8",
    "_WindowUnderflow12",
]

# `40083956:\t0c8976        \tloop\ta9, 40083966 <sym+0x6a>` — the mnemonic,
# then the register, then the end address objdump already resolved.
LOOP_RE = re.compile(
    r"^\s*([0-9a-f]+):\s+([0-9a-f ]+)\s+(loop|loopnez|loopgtz)\s+\S+,\s*([0-9a-f]+)"
)
SECTION_RE = re.compile(r"^Disassembly of section (\S+):")


def read_dump(path):
    """Parse the `[run]`/`[cores]`/`[mmio]`/`[fetch]` sections."""
    run, cores, mmio, fetch = {}, [], [], []
    meta = {}
    section = None
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            if line.startswith("["):
                section = line.strip("[]")
                continue
            if line.startswith("#"):
                # `# total 3550668`, `# ram_reads N ram_writes M`, `# fetches N`
                parts = line[1:].split()
                for i in range(0, len(parts) - 1, 2):
                    if parts[i + 1].isdigit():
                        meta[parts[i]] = int(parts[i + 1])
                continue
            parts = line.split()
            if section == "run":
                run[parts[0]] = int(parts[1])
            elif section == "cores":
                cores.append([int(p) for p in parts[:4]])
            elif section == "mmio":
                mmio.append(
                    (int(parts[0], 16), int(parts[1]), int(parts[2]), parts[3], parts[4])
                )
            elif section == "fetch":
                fetch.append((int(parts[0], 16), int(parts[1])))
    return run, cores, mmio, fetch, meta


def tool(name):
    path = shutil.which(name)
    if not path:
        sys.exit(
            f"bench-esp32v3-counts: {name} is not on PATH — "
            "add `just _xt-gcc-dir xtensa-esp32-elf-gcc` to it"
        )
    return path


def window_vectors(elf):
    out = subprocess.run(
        [tool("xtensa-esp32-elf-nm"), elf], capture_output=True, text=True, check=True
    ).stdout
    found = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] in WINDOW_SYMBOLS:
            found[parts[2]] = int(parts[0], 16)
    missing = [s for s in WINDOW_SYMBOLS if s not in found]
    if missing:
        sys.exit(f"bench-esp32v3-counts: the image has no {', '.join(missing)}")
    return found


def loop_bodies_and_extents(elf):
    """Every zero-overhead loop body, plus the address extent objdump covered.

    The extent is what separates "this pc is not in a loop" from "this pc is
    not in the image at all" — the JIT's code is the second.
    """
    out = subprocess.run(
        [tool("xtensa-esp32-elf-objdump"), "-d", elf],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    bodies = []
    covered = []
    section = None
    lo = hi = None
    for line in out.splitlines():
        m = SECTION_RE.match(line)
        if m:
            if section is not None and lo is not None:
                covered.append((section, lo, hi))
            section, lo, hi = m.group(1), None, None
            continue
        m = re.match(r"^\s*([0-9a-f]+):\s", line)
        if m:
            a = int(m.group(1), 16)
            lo = a if lo is None else min(lo, a)
            hi = a if hi is None else max(hi, a)
        m = LOOP_RE.match(line)
        if m:
            start = int(m.group(1), 16)
            width = len(m.group(2).replace(" ", "")) // 2
            end = int(m.group(4), 16)
            # The body is everything after the `loop` instruction itself up to
            # (not including) the end address the instruction names.
            bodies.append((start + width, end, start))
    if section is not None and lo is not None:
        covered.append((section, lo, hi))
    return bodies, covered


def in_any(ranges, pc):
    for lo, hi in ranges:
        if lo <= pc < hi:
            return True
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("counts")
    ap.add_argument("--elf", required=True)
    ap.add_argument("--top", type=int, default=15)
    ap.add_argument("--markdown", action="store_true")
    args = ap.parse_args()

    run, cores, mmio, fetch, meta = read_dump(args.counts)
    instructions = run.get("instructions", 0)
    fetches = meta.get("fetches", 0)
    mmio_total = meta.get("total", 0)
    ram_reads = meta.get("ram_reads", 0)
    ram_writes = meta.get("ram_writes", 0)

    print(f"image   {args.elf}")
    print(f"counts  {args.counts}")
    print(
        f"run     {instructions:,} instructions, {run.get('cycles', 0):,} cycles, "
        f"{run.get('idle_skips', 0):,} idle skips, quantum {run.get('quantum', 0)}"
    )
    print()

    # --- 1. MMIO -----------------------------------------------------------
    data = ram_reads + ram_writes + mmio_total
    print("## MMIO")
    print(
        f"MMIO accesses {mmio_total:,} — {100 * mmio_total / data:.2f}% of the "
        f"{data:,} data accesses, {1e6 * mmio_total / instructions:.0f} per "
        "million instructions"
    )
    print(f"RAM loads {ram_reads:,}, RAM stores {ram_writes:,}")
    print()
    header = "| site | block | register | reads | writes | share of MMIO |"
    print(header)
    print("|---|---|---|---:|---:|---:|")
    for address, reads, writes, block, reg in mmio[: args.top]:
        share = 100 * (reads + writes) / mmio_total if mmio_total else 0
        print(
            f"| `{address:#010x}` | {block} | {reg} | {reads:,} | {writes:,} | "
            f"{share:.2f}% |"
        )
    print()

    # --- 2. window exceptions ---------------------------------------------
    vectors = window_vectors(args.elf)
    by_pc = dict(fetch)
    print("## Window exceptions")
    print("| handler | address | entries | per M instructions |")
    print("|---|---|---:|---:|")
    total_w = 0
    for sym in WINDOW_SYMBOLS:
        addr = vectors[sym]
        n = by_pc.get(addr, 0)
        total_w += n
        print(
            f"| `{sym}` | `{addr:#010x}` | {n:,} | "
            f"{1e6 * n / instructions:.1f} |"
        )
    over = sum(by_pc.get(vectors[s], 0) for s in WINDOW_SYMBOLS if "Overflow" in s)
    under = total_w - over
    print(
        f"\noverflow {over:,}, underflow {under:,}, total {total_w:,} — "
        f"{1e6 * total_w / instructions:.1f} window exceptions per million "
        f"instructions ({100 * total_w / instructions:.4f}% of retired "
        "instructions are a handler's first)"
    )
    print()

    # --- 3. LOOP hotness ---------------------------------------------------
    bodies, covered = loop_bodies_and_extents(args.elf)
    ranges = sorted((lo, hi) for lo, hi, _ in bodies)
    cov = sorted((lo, hi + 4) for _, lo, hi in covered)
    in_loop = 0
    in_image = 0
    outside = 0
    outside_by_64k = {}
    for pc, n in fetch:
        if in_any(cov, pc):
            in_image += n
            if in_any(ranges, pc):
                in_loop += n
        else:
            outside += n
            outside_by_64k[pc >> 16] = outside_by_64k.get(pc >> 16, 0) + n
    print("## LOOP hotness")
    print(
        f"{len(bodies)} zero-overhead loop bodies in the image "
        f"({sum(hi - lo for lo, hi in ranges)} bytes of body)"
    )
    print(
        f"retired inside a loop body {in_loop:,} — "
        f"{100 * in_loop / fetches:.2f}% of all retired instructions, "
        f"{100 * in_loop / in_image:.2f}% of those that ran from the image"
    )
    print(
        f"retired from the image {in_image:,} ({100 * in_image / fetches:.2f}%), "
        f"OUTSIDE it {outside:,} ({100 * outside / fetches:.2f}%) — the JIT's "
        "code, which no static scan can classify"
    )
    print()
    # Where "outside" actually is. The mask ROM is outside the firmware ELF
    # too, and so is anything the loader placed that objdump would not
    # disassemble — naming the 64 KiB pages is what separates "the JIT" from
    # "the ROM" without either being assumed.
    print("| outside-the-image 64 KiB page | instructions | share of run |")
    print("|---|---:|---:|")
    for page, n in sorted(outside_by_64k.items(), key=lambda kv: -kv[1])[: args.top]:
        print(
            f"| `{page << 16:#010x}` | {n:,} | {100 * n / fetches:.2f}% |"
        )
    print()

    # --- 4. the interleave -------------------------------------------------
    print("## The interleave")
    print("| core | instructions | share | windows | instr/window | waiti parks |")
    print("|---|---:|---:|---:|---:|---:|")
    total_windows = 0
    for core, instr, windows, wfi in cores:
        total_windows += windows
        per = instr / windows if windows else 0
        print(
            f"| {core} | {instr:,} | {100 * instr / instructions:.1f}% | "
            f"{windows:,} | {per:.1f} | {wfi:,} |"
        )
    print(
        f"\n{total_windows:,} windows over {instructions:,} instructions — "
        f"{1e6 * total_windows / instructions:.0f} core switches per million "
        f"instructions, one every {instructions / total_windows:.0f} "
        f"instructions; the quantum is {run.get('quantum', 0)} cycles, so a "
        "window that retires fewer than that ended early (a bus yield, a "
        "scheduled event, a host service or a `waiti`)."
    )


if __name__ == "__main__":
    main()

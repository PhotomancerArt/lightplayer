#!/usr/bin/env python3
"""MMIO and ROM-call census of an Xtensa firmware image.

Why this exists
---------------
Before a machine exists, the only way to know which peripheral blocks it has
to answer is to read them out of the image. `census.py` answers "can the
decoder read this?" and `sweep.py` answers "would a discovery walk mis-decode
it?"; this script answers the two questions a *machine* is built from:

1. **Which peripheral blocks does the image touch, and at which offsets?**
   Bucketed against the chip PAC's own `Periph<..., 0xBASE>` declarations —
   never a datasheet — and attributed to the symbol that does the touching.
2. **Which mask-ROM entry points does it call?** Resolved against the
   `PROVIDE`d symbols in `esp-rom-sys`'s linker scripts and reported as
   GROUPS, never as a single "nearest" name.

Both questions are answered from `l32r`, because that is how an Xtensa image
names a 32-bit address: `movi` holds twelve signed bits and cannot reach
`0x6000_0000` or `0x4000_0000`. objdump resolves every `l32r`'s literal for
us and prints the value in parentheses, so the literal pool never has to be
read separately.

⚠️ **A loaded constant is not an access.** Half the point of this script is
the false-positive filter: `0x6000_0020` is UART0's `int_raw` *and* a plain
32-bit constant that shows up in JSON and shader code, and an image spills
plenty of those to the stack. So a peripheral-shaped literal counts as MMIO
only when the register it was loaded into is actually dereferenced by an
`l32i`/`s32i` inside the same symbol before it is redefined. Literals that
are never dereferenced are reported separately, by value, so the filter can
be audited rather than believed.

⚠️ **Nearest-symbol resolution lies on the ROM.** Multiple `PROVIDE`s share
addresses in the S3 ROM linker scripts — `0x4000_1c68` is BOTH
`r_llc_rem_phy_upd_proc_continue_hook` and `MD5Update` — so this script
resolves only EXACT addresses and prints every name an address has. An
address with no exact match is reported as unresolved, never snapped to the
nearest symbol below it.

Licence note (AGENTS.md): binutils (`objdump`, `nm`) is used here as a tool
whose *output* is fact — disassembly text and the symbol table. No binutils
source, table or logic is read or adapted. The PAC bases and the ROM symbol
addresses are read from the crates' own generated sources, the same way
`pac-regnames.py` reads register offsets.

Usage
-----
    scripts/emu/xtensa-inventory/mmio-census.py <artefact.elf> --chip esp32s3
        [--objdump PATH] [--nm PATH] [--pac-src DIR] [--rom-ld DIR]
        [--json FILE] [--top N]

`--chip` picks the toolchain prefix, the PAC crate and the ROM linker-script
directory. Everything else is an override for a machine whose cargo registry
is somewhere unusual.
"""

from __future__ import annotations

import argparse
import bisect
import glob
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

DEFAULT_TOOLCHAIN = Path(
    os.path.expanduser(
        "~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin"
    )
)

# One entry per chip this script knows. `pac`/`pac_version` are the crate the
# workspace `Cargo.lock` pins; `rom_ld` is the directory of `PROVIDE`d ROM
# symbols inside `esp-rom-sys`. `periph` and `rom` are the address windows
# each question looks in.
CHIPS = {
    "esp32s3": {
        "prefix": "xtensa-esp32s3-elf",
        "pac": "esp32s3",
        "pac_version": "0.35.2",
        "rom_crate": "esp-rom-sys-0.1.4",
        "rom_ld": "ld/esp32s3/rom",
        # The S3's peripheral space, plus the two RTC windows. RTC fast is
        # memory rather than registers but the image reaches it the same way
        # (`.rtc_fast.persistent`), so it is counted and labelled.
        "periph": [
            (0x6000_0000, 0x6010_0000, "peripheral"),
            (0x600F_E000, 0x6010_0000, "rtc-fast"),
            (0x5000_0000, 0x5000_2000, "rtc-slow"),
        ],
        "rom": (0x4000_0000, 0x4006_0000),
        # A PAC base claims addresses up to this far above it. Without a cap,
        # `bisect` hands RTC fast (`0x600f_e000`) to WCL (`0x600d_0000`, the
        # highest base below it) and invents a `WCL+0x2e000` that is not a
        # register anywhere. 4 KB is the S3's peripheral block stride.
        "block_span": 0x1000,
        # Windows the PAC has no `Periph` type for. RTC fast is memory, not
        # registers — the image reaches it through `.rtc_fast.persistent` —
        # but it is reached the same way and a machine has to map it.
        "extra": [
            (0x600F_E000, 0x6010_0000, "RTC_FAST (memory)"),
            (0x5000_0000, 0x5000_2000, "RTC_SLOW (memory)"),
        ],
        # Two PAC types at one base. `INTERRUPT_CORE0` is `+0x000..+0x800`
        # and `INTERRUPT_CORE1` is `+0x800..` INSIDE the same 4 KB window
        # (M6 notes §3.3), so the bucket is split by offset rather than by
        # base or the report would show one block with both cores' registers.
        "splits": {
            0x600C_2000: [(0x000, 0x800, "INTERRUPT_CORE0"), (0x800, 0x1000, "INTERRUPT_CORE1")],
        },
    },
}

# `pub type RTC_CNTL = crate::Periph<rtc_cntl::RegisterBlock, 0x6000_8000>;`
_PERIPH_RE = re.compile(
    r"pub type ([A-Z0-9_]+) = crate::Periph<\s*([a-z0-9_]+)::RegisterBlock,\s*"
    r"(0x[0-9a-fA-F_]+)\s*>"
)
# `PROVIDE( Cache_Suspend_DCache = 0x400018b4 );` and the bare
# `MD5Update = 0x40001c68;` form, which the BLE scripts use.
_PROVIDE_RE = re.compile(
    r"^\s*(?:PROVIDE\s*\(\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(0x[0-9a-fA-F]+)"
)
# objdump -d line: "4004f6f7:\tf39691  \tl32r\ta9, 4004c550 <sym+0x30> (600c5000 <sym>)"
_LINE_RE = re.compile(r"^\s*([0-9a-f]+):\t")
# `l32r a9, 4004c550 <...> (600c5000 <...>)` — the parenthesised value is the
# literal objdump read out of the pool for us.
_L32R_RE = re.compile(r"l32r\s+a(\d+),\s+[0-9a-f]+\s+<[^>]*>\s+\((?:0x)?([0-9a-f]+)\b")
# `l32i.n a10, a9, 0` / `l32i a2, a3, 0x1c` / `s32i a8, a9, 40` / `l8ui a12,
# a11, 0` / `s8i a12, a11, 0`. ALL widths, not just 32-bit: the four
# RF-adjacent blocks `esp_hal::init` touches are BYTE accesses (`l8ui` +
# `and` + `s8i` on `NRX+0xd4` and `FE+0x90`), and a 32-bit-only filter reports
# them as untouched — which is exactly the kind of block a strict bring-up
# then meets as a surprise stop.
_MEM_RE = re.compile(
    r"\b(l32i(?:\.n)?|s32i(?:\.n)?|l8ui|s8i|l16ui|l16si|s16i|l32ai|s32ri)"
    r"\s+a\d+,\s+a(\d+),\s+(-?(?:0x)?[0-9a-fA-F]+)"
)
# `call8 4004f664 <...>` / `j 4004f78b <...>` / `call0 40001a1c <...>`
_CALL_RE = re.compile(r"\b(call0|call4|call8|call12|j)\s+([0-9a-f]+)\b")
# nm -S -n: "400d0020 00000018 t sym_name"
_NM_RE = re.compile(r"^([0-9a-f]+)(?: ([0-9a-f]+))? (\S) (.+)$")
# Any instruction whose first operand is `aN` — the destination for everything
# except the stores, which are handled before this is consulted.
_DEST_RE = re.compile(r"\b[a-z0-9_.]+\s+a(\d+),")
# Address arithmetic that CARRIES a pointer rather than destroying it:
# `addx4 a8, a5, a8` is how esp-hal indexes the GPIO matrix, and a tracker
# that treated it as a redefinition would drop `GPIO+0x554` — which is a real
# register the image writes — into the false-positive pile.
#
# Only INDEXING and REGISTER-MOVE forms are here, and that line is drawn
# deliberately. `add`/`add.n`/`sub`/`or`/`and` on a peripheral-shaped constant
# is what plain 32-bit arithmetic looks like — `0x6000_0000` appears in this
# image inside `serde_json`'s float formatter and littlefs's block arithmetic,
# reached by `add.n`, and carrying the pointer through those turns a constant
# into a phantom `UART0+0x0c` the image never touches. `addx4` is how esp-hal
# indexes the GPIO matrix and dropping IT loses the real `GPIO+0x554`.
_ADDR_ARITH_RE = re.compile(
    r"\b(addx1|addx2|addx4|addx8|addi\.n|addi|addmi|mov\.n|mov)"
    r"\s+a(\d+),\s+a(\d+)(?:,\s*(a\d+|-?(?:0x)?[0-9a-fA-F]+))?\s*$"
)


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"mmio-census: failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}")
    return proc.stdout


def cargo_home() -> str:
    return os.environ.get("CARGO_HOME", os.path.join(os.path.expanduser("~"), ".cargo"))


def find_crate(stem: str) -> str | None:
    dirs = glob.glob(os.path.join(cargo_home(), "registry", "src", "*", stem))
    return dirs[0] if dirs else None


def pac_bases(chip: dict, override: str | None) -> list[tuple[int, str, str]]:
    """[(base, STATIC, module), ...] from the PAC's own `peripherals!` types.

    Sorted by base. Several peripherals can share a base (the S3's
    `INTERRUPT_CORE0`/`INTERRUPT_CORE1` do, being two halves of one window);
    the bucketing keeps the first name and the report says so.
    """
    root = override or find_crate(f"{chip['pac']}-{chip['pac_version']}")
    if root is None:
        sys.exit(
            f"mmio-census: the {chip['pac']} {chip['pac_version']} sources are not in\n"
            "    this machine's cargo registry. Run `cargo fetch --locked` first, or\n"
            "    pass --pac-src."
        )
    text = Path(root, "src", "lib.rs").read_text(encoding="utf-8")
    out = [
        (int(m.group(3).replace("_", ""), 16), m.group(1), m.group(2))
        for m in _PERIPH_RE.finditer(text)
    ]
    if not out:
        sys.exit(f"mmio-census: no `Periph<...>` types in {root}/src/lib.rs")
    out.sort()
    return out


def rom_symbols(chip: dict, override: str | None) -> dict[int, list[str]]:
    """addr -> every name the linker scripts give it. Exact matches only."""
    root = override or find_crate(chip["rom_crate"])
    if root is None:
        sys.exit(
            f"mmio-census: {chip['rom_crate']} is not in this machine's cargo\n"
            "    registry. Run `cargo fetch --locked` first, or pass --rom-ld."
        )
    ld_dir = override if override and os.path.isdir(override) else os.path.join(root, chip["rom_ld"])
    out: dict[int, list[str]] = defaultdict(list)
    files = sorted(glob.glob(os.path.join(ld_dir, "*.ld")))
    if not files:
        sys.exit(f"mmio-census: no .ld files under {ld_dir}")
    for path in files:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            m = _PROVIDE_RE.match(line)
            if not m:
                continue
            name, addr = m.group(1), int(m.group(2), 16)
            if name not in out[addr]:
                out[addr].append(name)
    return dict(out)


def symbol_ranges(nm: Path, elf: Path) -> list[tuple[int, int, str]]:
    out = []
    for line in run([str(nm), "-S", "-n", "--defined-only", str(elf)]).splitlines():
        m = _NM_RE.match(line.strip())
        if not m or m.group(2) is None:
            continue
        addr, size, kind = int(m.group(1), 16), int(m.group(2), 16), m.group(3)
        if size == 0 or kind.lower() not in ("t", "w"):
            continue
        out.append((addr, addr + size, m.group(4)))
    out.sort()
    return out


def enclosing(addr: int, starts: list[int], ranges: list[tuple[int, int, str]]) -> str | None:
    i = bisect.bisect_right(starts, addr) - 1
    if i < 0:
        return None
    start, end, name = ranges[i]
    return name if start <= addr < end else None


def bucket(
    addr: int, bases: list[tuple[int, str, str]], chip: dict
) -> tuple[int, str, str] | None:
    """`(base, name, module)` for `addr`, or None if it is in no known block.

    The PAC peripheral whose base is the highest one at or below `addr`, but
    only if `addr` is inside that block's span — otherwise the chip's `extra`
    windows, and otherwise nothing. Returning None is the point: an address in
    no block is a finding for the report, never a register invented by
    rounding down to whatever base happened to be nearest.
    """
    i = bisect.bisect_right([b for b, _, _ in bases], addr) - 1
    if i >= 0:
        base, name, module = bases[i]
        if addr - base < chip["block_span"]:
            for lo, hi, split_name in chip.get("splits", {}).get(base, []):
                if lo <= addr - base < hi:
                    return (base + lo, split_name, module)
            return (base, name, module)
    for lo, hi, label in chip.get("extra", []):
        if lo <= addr < hi:
            return (lo, label, "-")
    return None


def in_window(addr: int, windows: list[tuple[int, int, str]]) -> str | None:
    for lo, hi, label in windows:
        if lo <= addr < hi:
            return label
    return None


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("artefact", type=Path)
    ap.add_argument("--chip", choices=sorted(CHIPS), default="esp32s3")
    ap.add_argument("--objdump", type=Path)
    ap.add_argument("--nm", type=Path)
    ap.add_argument("--pac-src")
    ap.add_argument("--rom-ld")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--top", type=int, default=25)
    args = ap.parse_args()

    chip = CHIPS[args.chip]
    objdump = args.objdump or DEFAULT_TOOLCHAIN / f"{chip['prefix']}-objdump"
    nm = args.nm or DEFAULT_TOOLCHAIN / f"{chip['prefix']}-nm"

    bases = pac_bases(chip, args.pac_src)
    roms = rom_symbols(chip, args.rom_ld)
    syms = symbol_ranges(nm, args.artefact)
    sym_starts = [s for s, _, _ in syms]

    print(f"=== xtensa-inventory MMIO + ROM-call census: {args.artefact} ===")
    print(f"chip: {args.chip}   objdump: {objdump}")
    print(f"PAC bases: {len(bases)} from {chip['pac']}-{chip['pac_version']}/src/lib.rs")
    print(f"ROM symbols: {sum(len(v) for v in roms.values())} names at {len(roms)} addresses")
    print(f"symbols (defined, sized, code): {len(syms)}")
    print()

    listing = run([str(objdump), "-d", str(args.artefact)]).splitlines()

    # --- pass 1: walk the listing, tracking live peripheral/ROM pointers ----
    #
    # `live[reg] = (value, site_addr, symbol)` for a register an `l32r` has
    # just loaded with an address in a window we care about. A later
    # `l32i`/`s32i` through that register is a dereference; any other write to
    # it kills the tracking. The map is cleared at every symbol boundary, so a
    # pointer is never carried across functions.
    live: dict[int, dict] = {}
    cur_sym: str | None = None

    periph_sites: list[dict] = []  # every peripheral-shaped l32r
    rom_refs: Counter = Counter()  # rom addr -> reference count
    rom_callers: dict[int, set[str]] = defaultdict(set)
    rom_direct: Counter = Counter()  # rom addr -> direct call/jump count

    for line in listing:
        m = _LINE_RE.match(line)
        if not m:
            continue
        addr = int(m.group(1), 16)
        sym = enclosing(addr, sym_starts, syms)
        if sym != cur_sym:
            live.clear()
            cur_sym = sym

        lm = _L32R_RE.search(line)
        if lm:
            reg, value = int(lm.group(1)), int(lm.group(2), 16)
            if in_window(value, chip["periph"]):
                rec = {"value": value, "site": addr, "symbol": sym, "accesses": []}
                periph_sites.append(rec)
                live[reg] = rec
            elif chip["rom"][0] <= value < chip["rom"][1]:
                rom_refs[value] += 1
                rom_callers[value].add(sym or "<before any symbol>")
                live.pop(reg, None)
            else:
                live.pop(reg, None)
            continue

        mm = _MEM_RE.search(line)
        if mm:
            base_reg = int(mm.group(2))
            rec = live.get(base_reg)
            if rec is not None:
                off_s = mm.group(3)
                off = int(off_s, 16) if off_s.lower().startswith(("0x", "-0x")) else int(off_s)
                rec["accesses"].append((mm.group(1), off))
            # A LOAD redefines its destination; a STORE's first operand is the
            # source and must not be untracked.
            if mm.group(1)[0] == "l":
                dm = _DEST_RE.search(line)
                if dm:
                    live.pop(int(dm.group(1)), None)
            continue

        cm = _CALL_RE.search(line)
        if cm:
            target = int(cm.group(2), 16)
            if chip["rom"][0] <= target < chip["rom"][1]:
                rom_direct[target] += 1
                rom_refs[target] += 1
                rom_callers[target].add(sym or "<before any symbol>")
            continue

        # Address arithmetic carries the pointer into its destination; every
        # other write to a register ends its life as one.
        am = _ADDR_ARITH_RE.search(line)
        if am:
            dest, src = int(am.group(2)), int(am.group(3))
            src2 = am.group(4)
            carried = live.get(src)
            if carried is None and src2 is not None and src2.startswith("a"):
                carried = live.get(int(src2[1:]))
            if carried is not None:
                live[dest] = carried
            else:
                live.pop(dest, None)
            continue

        dm = _DEST_RE.search(line)
        if dm:
            live.pop(int(dm.group(1)), None)

    # --- the MMIO table ----------------------------------------------------
    real = [r for r in periph_sites if r["accesses"]]
    false_pos = [r for r in periph_sites if not r["accesses"]]

    # The block table buckets EVERY peripheral-shaped literal, not only the
    # ones whose dereference this script could follow. A linear tracker loses
    # a pointer the moment it is spilled, passed to a helper or reloaded after
    # a branch, and `SYSTEM+0x030` (`cpu_intr_from_cpu0`), `RMT+0x800` (the
    # channel RAM) and the interrupt-map bases are all real registers it loses
    # that way. Under-reporting a block is the expensive mistake here: a block
    # nobody named becomes a surprise stop in somebody's strict bring-up,
    # while a block named once too often costs one accept entry. The
    # dereference evidence is kept as a per-block column and in the audit list
    # further down, so the filter can be checked rather than believed.
    blocks: dict[int, dict] = {}
    unbucketed: set[int] = set()

    def touch(addr: int, rec: dict, derefed: bool) -> None:
        b = bucket(addr, bases, chip)
        if b is None:
            unbucketed.add(addr)
            return
        base, static, module = b
        e = blocks.setdefault(
            base,
            {"base": base, "names": [], "module": module, "offsets": set(),
             "sites": 0, "deref": 0, "symbols": set()},
        )
        if static not in e["names"]:
            e["names"].append(static)
        e["offsets"].add(addr - base)
        e["symbols"].add(rec["symbol"] or "<before any symbol>")
        if derefed:
            e["deref"] += 1

    for rec in periph_sites:
        touch(rec["value"], rec, bool(rec["accesses"]))
        for _kind, off in rec["accesses"]:
            touch(rec["value"] + off, rec, True)
        b = bucket(rec["value"], bases, chip)
        if b is not None:
            blocks[b[0]]["sites"] += 1

    print(f"--- peripheral-shaped `l32r` literals: {len(periph_sites)} "
          f"({len(real)} with a dereference this script could follow, "
          f"{len(false_pos)} without) ---")
    print()
    print(f"{'block':24} {'base':>12} {'regs':>5} {'sites':>6} {'deref':>6}  offsets")
    for base in sorted(blocks):
        e = blocks[base]
        offs = ",".join(f"{o:02x}" for o in sorted(e["offsets"]))
        name = "/".join(e["names"])
        print(f"{name:24} {base:#012x} {len(e['offsets']):>5} {e['sites']:>6} "
              f"{e['deref']:>6}  +0x{offs}")
    print()
    if unbucketed:
        print("--- peripheral-window addresses in NO PAC block and no named region ---")
        for a in sorted(unbucketed):
            print(f"    {a:#010x}")
        print()

    for base in sorted(blocks):
        e = blocks[base]
        print(f"{'/'.join(e['names'])} ({e['module']}) touched by:")
        for s in sorted(e["symbols"]):
            print(f"    {s}")
    print()

    print("--- literals that LOOK like peripherals and are never dereferenced ---")
    fp = Counter(r["value"] for r in false_pos)
    for value, n in fp.most_common():
        b = bucket(value, bases, chip)
        label = f"{b[1]}+{value - b[0]:#05x}" if b else "?"
        print(f"    {value:#010x}  x{n:<4}  (would have read as {label})")
    print()

    # --- the ROM-call inventory -------------------------------------------
    print(f"--- mask-ROM entry points referenced: {len(rom_refs)} distinct, "
          f"{sum(rom_refs.values())} references ---")
    print()
    unresolved = []
    print(f"{'address':>12} {'refs':>7} {'direct':>7}  names (EVERY name at this address)")
    for addr, n in rom_refs.most_common():
        names = roms.get(addr)
        if names is None:
            unresolved.append((addr, n))
            continue
        print(f"{addr:#012x} {n:>7} {rom_direct.get(addr, 0):>7}  {', '.join(names)}")
    print()
    if unresolved:
        print("--- ROM-range literals with NO exact linker-script symbol ---")
        print("    (reported, never snapped to the nearest symbol below them:")
        print("     a nearest-symbol name on this ROM is a guess, not a resolution)")
        for addr, n in unresolved:
            print(f"    {addr:#010x}  x{n}")
        print()

    print("--- top ROM callers by reference count ---")
    for addr, n in rom_refs.most_common(args.top):
        names = roms.get(addr, ["<unresolved>"])
        print(f"    {n:>6}  {names[0]:<40} {len(rom_callers[addr])} caller symbols")

    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "artefact": str(args.artefact),
                    "chip": args.chip,
                    "periph_literal_sites": len(periph_sites),
                    "periph_dereferenced": len(real),
                    "periph_false_positives": {hex(v): n for v, n in fp.most_common()},
                    "blocks": [
                        {
                            "names": blocks[b]["names"],
                            "module": blocks[b]["module"],
                            "base": hex(b),
                            "offsets": [hex(o) for o in sorted(blocks[b]["offsets"])],
                            "deref": blocks[b]["deref"],
                            "sites": blocks[b]["sites"],
                            "symbols": sorted(blocks[b]["symbols"]),
                        }
                        for b in sorted(blocks)
                    ],
                    "rom": [
                        {
                            "addr": hex(a),
                            "refs": n,
                            "direct": rom_direct.get(a, 0),
                            "names": roms.get(a, []),
                            "callers": len(rom_callers[a]),
                        }
                        for a, n in rom_refs.most_common()
                    ],
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        print(f"\nwrote {args.json}")


if __name__ == "__main__":
    main()

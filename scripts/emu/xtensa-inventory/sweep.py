#!/usr/bin/env python3
"""Literal-pool collision sweep for a symbol-seeded, width-following discovery
walk over an Xtensa firmware image (scope-isa-study.md study §2.3 / §4.4).

Why this exists
----------------
`l32r` loads a 32-bit constant from a literal pool at a *negative*
PC-relative offset, so literal pools are interleaved with code — sitting
immediately before the functions that reference them. A discovery walk that
follows decoded instruction widths forward from a function's start, without
a known stop point, can run past that function's real end and into the next
literal pool, decoding constants as if they were instructions.

This script does not implement a decoder. It uses GNU objdump's own,
already-correct disassembly (ground truth: every real instruction start
address and width in the image) and asks a narrower question: for each
instruction address decoded inside `.text`, does it fall *inside* the
`[sym, sym+size)` range of the ELF symbol whose start it comes after, and is
it *outside* any `.literal`/`.rodata` section? An address that fails either
check is a byte a naive width-following sweep — seeded at that symbol and
walking forward with no boundary information — would have wrongly decoded.

Licence note (AGENTS.md): objdump is used here as a tool whose *output* is
fact (disassembly addresses, section headers, symbol table). No binutils
source, table, or logic is read or adapted.

Usage
-----
    scripts/emu/xtensa-inventory/sweep.py <artefact.elf> [--objdump PATH] [--nm PATH]
"""

from __future__ import annotations

import argparse
import bisect
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

DEFAULT_TOOLCHAIN = Path(
    os.path.expanduser(
        "~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin"
    )
)

# objdump -d instruction line, tab-delimited: "400d0020:\t572c      \tmovi.n\ta7, 37"
# The hex-bytes field has NO spaces between byte pairs on this objdump
# (verified by hand: "1c4012" is one 3-byte instruction, not three fields) —
# so width is (hex digit count)/2, matching objdiff.rs's own parse_line.
# objdump -h section line: "  4 .literal      00000a10  3ffb7f90  3ffb7f90  00027f90  2**2"
_SEC_RE = re.compile(
    r"^\s*\d+\s+(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+2\*\*"
)
# nm -S -n line: "400d0020 00000018 t sym_name"  (size field may be absent)
_NM_RE = re.compile(r"^([0-9a-f]+)(?: ([0-9a-f]+))? (\S) (.+)$")

LITERAL_SECTION_PREFIXES = (".literal", ".rodata")


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"sweep: command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}")
    return proc.stdout


def code_sections(objdump: Path, elf: Path) -> list[tuple[str, int, int]]:
    lines = run([str(objdump), "-h", str(elf)]).splitlines()
    out = []
    for i, line in enumerate(lines):
        m = _SEC_RE.match(line)
        if not m:
            continue
        flags = lines[i + 1] if i + 1 < len(lines) else ""
        if "CODE" not in flags:
            continue
        name, size, vma = m.group(1), int(m.group(2), 16), int(m.group(3), 16)
        if size:
            out.append((name, size, vma))
    return out


def literal_ranges(objdump: Path, elf: Path) -> list[tuple[int, int, str]]:
    """[(start, end, name), ...] for every `.literal*`/`.rodata*` section."""
    lines = run([str(objdump), "-h", str(elf)]).splitlines()
    out = []
    for line in lines:
        m = _SEC_RE.match(line)
        if not m:
            continue
        name, size, vma = m.group(1), int(m.group(2), 16), int(m.group(3), 16)
        if size and name.startswith(LITERAL_SECTION_PREFIXES):
            out.append((vma, vma + size, name))
    out.sort()
    return out


def symbol_ranges(nm: Path, elf: Path) -> list[tuple[int, int, str]]:
    """[(start, end, name), ...] for defined FUNC/object symbols with size>0, sorted by start."""
    out = []
    text = run([str(nm), "-S", "-n", "--defined-only", str(elf)])
    for line in text.splitlines():
        m = _NM_RE.match(line.strip())
        if not m:
            continue
        addr_s, size_s, kind, name = m.groups()
        if size_s is None:
            continue
        addr = int(addr_s, 16)
        size = int(size_s, 16)
        if size == 0:
            continue
        # lowercase kind = local, uppercase = global; both are real symbols.
        if kind.lower() not in ("t", "w"):  # text (code) symbols only
            continue
        out.append((addr, addr + size, name))
    out.sort()
    return out


def decoded_instructions(objdump: Path, elf: Path, section: str) -> list[tuple[int, int]]:
    """[(addr, width_bytes), ...] for every decoded line in `section`, in address order.

    Parses the same way `objdiff.rs::parse_line` does: split the address off
    at the first `:`, then split the rest on tabs — the first non-empty
    tab-field is the hex-bytes field (its hex-digit count / 2 is the width),
    the remaining fields are the mnemonic/operands. This intentionally
    includes `.byte` data-directive lines (mis-decoded regions, e.g. the
    `.vectors` section) — they are real bytes a naive sweep would still walk
    through, so they belong in the collision count, not excluded from it.
    """
    text = run([str(objdump), "-d", "-j", section, str(elf)])
    out = []
    for line in text.splitlines():
        if ":\t" not in line:
            continue
        addr_part, rest = line.split(":", 1)
        addr_part = addr_part.strip()
        if not addr_part or any(c not in "0123456789abcdef" for c in addr_part):
            continue
        fields = [f for f in rest.split("\t") if f.strip()]
        if not fields:
            continue
        hexdigits = "".join(c for c in fields[0] if c in "0123456789abcdef")
        if not hexdigits:
            continue
        addr = int(addr_part, 16)
        width = len(hexdigits) // 2
        out.append((addr, width))
    return out


def enclosing_symbol(addr: int, starts: list[int], ranges: list[tuple[int, int, str]]) -> tuple[int, int, str] | None:
    i = bisect.bisect_right(starts, addr) - 1
    if i < 0:
        return None
    start, end, name = ranges[i]
    if start <= addr < end:
        return ranges[i]
    return None


def in_literal(addr: int, lits: list[tuple[int, int, str]]) -> bool:
    starts = [s for s, _, _ in lits]
    i = bisect.bisect_right(starts, addr) - 1
    if i < 0:
        return False
    s, e, _ = lits[i]
    return s <= addr < e


def preceding_symbol(addr: int, starts: list[int], ranges: list[tuple[int, int, str]]) -> str:
    i = bisect.bisect_right(starts, addr) - 1
    if i < 0:
        return "<before any symbol>"
    return ranges[i][2]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("artefact", type=Path)
    ap.add_argument("--objdump", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-objdump")
    ap.add_argument("--nm", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-nm")
    ap.add_argument("--top", type=int, default=20)
    args = ap.parse_args()

    secs = code_sections(args.objdump, args.artefact)
    lits = literal_ranges(args.objdump, args.artefact)
    syms = symbol_ranges(args.nm, args.artefact)
    sym_starts = [s for s, _, _ in syms]

    total_insns = 0
    collision_insns = 0
    collision_bytes = 0
    worst: Counter = Counter()  # preceding symbol name -> collision bytes

    for name, _size, _vma in secs:
        for addr, width in decoded_instructions(args.objdump, args.artefact, name):
            total_insns += 1
            enc = enclosing_symbol(addr, sym_starts, syms)
            bad_symbol = enc is None
            bad_literal = in_literal(addr, lits)
            if bad_symbol or bad_literal:
                collision_insns += 1
                collision_bytes += width
                worst[preceding_symbol(addr, sym_starts, syms)] += width

    print(f"=== xtensa-inventory literal-pool sweep: {args.artefact} ===")
    print(f"objdump: {args.objdump}")
    print(f"nm: {args.nm}")
    print(f"code sections swept: {', '.join(n for n, _, _ in secs)}")
    print(f"symbols (defined, sized, code): {len(syms)}")
    print(f"literal/rodata sections: {len(lits)} ({', '.join(n for _, _, n in lits) or 'none'})")
    print()
    print(f"total decoded instructions: {total_insns}")
    print(
        f"collision instructions (outside enclosing symbol's [addr,addr+size) "
        f"or inside .literal/.rodata): {collision_insns} "
        f"({100.0 * collision_insns / total_insns:.4f}% of decoded instructions)"
        if total_insns
        else "total decoded instructions: 0"
    )
    print(f"collision bytes: {collision_bytes}")
    print()
    print(f"--- worst symbols by collision bytes (top {args.top}) ---")
    for name, n in worst.most_common(args.top):
        print(f"  {n:>7}  {name}")


if __name__ == "__main__":
    main()

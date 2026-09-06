#!/usr/bin/env python3
"""Count the flash-resident literals each IRAM function loads.

The ISR-in-RAM rule (memory note `isr-path-in-ram-rule`) says the whole
interrupt-handler path must live in RAM. Putting a function in IRAM is only half
of that: on Xtensa every constant a function cannot encode as an immediate is
reached through `l32r`, which loads a word from a literal pool, and the pool can
sit in flash even when the code does not. A cache miss inside an ISR is exactly
the jitter the rule exists to prevent. Attributes cannot be trusted here — at
`opt-level=z` the compiler moves things and closures escape their section — so
this reads the linked image.

For every function in the image's RAM-resident text sections it walks the
disassembly and counts the `l32r`s whose loaded *value* points into the
flash-mapped rodata window. The output is a `function, count` table; the gate is
that no function's count grows against a committed baseline.

    scripts/iram-flash-literals.py <elf> [--objdump xtensa-esp32-elf-objdump]
                                         [--baseline <table>] [--write-baseline]

Exit status is 1 when a baseline is given and some function's count grew.

⚠️ The values come from objdump, which prints each `l32r`'s resolved literal in
parentheses when the literal address falls inside a loaded section:

    l32r a8, 400804c8 <sym_sidata+0x9c> (3ff560a4 <_rodata_end+0xb11de4>)

Reading that is both simpler and more trustworthy than re-deriving it from a
section dump. It also filters out the noise for free: `.rwtext` interleaves code
with its literal pools, objdump disassembles the pools too, and a pool word that
happens to decode as an `l32r` resolves to nothing and is skipped here.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

# The classic's flash-mapped constant window. esp-hal's `ld/esp32/memory.x` maps
# `drom_seg` at 0x3F40_0020 with a 4 MB span; a literal whose *value* lands in
# here is a pointer into flash, which is the cache miss we are hunting.
FLASH_RODATA_LO = 0x3F40_0000
FLASH_RODATA_HI = 0x3F80_0000

# Text sections that are RAM-resident on this chip. `.rwtext` is where esp-hal
# puts `#[ram]` functions and the interrupt plumbing; the vector sections are
# hand-written and carry their own literals.
RAM_TEXT_SECTIONS = (".rwtext", ".rwtext.wifi", ".vectors", ".iram0.text")

_SECTION_HEADER = re.compile(r"^\s*\d+\s+(?P<name>\S+)\s+[0-9a-f]{8}\s", re.I)
_FUNC_HEADER = re.compile(r"^(?P<addr>[0-9a-f]+)\s+<(?P<name>[^>]+)>:")
# `l32r a8, <literal addr> [<sym>] (<value> [<sym>])` — only the parenthesised
# value is of interest, and its presence is what proves the literal was real.
_L32R_VALUE = re.compile(r"\bl32r\b.*\(\s*(?P<value>[0-9a-f]+)\b", re.I)


def run(objdump: str, *args: str) -> str:
    return subprocess.run(
        [objdump, *args], check=True, capture_output=True, text=True
    ).stdout


def section_headers(objdump: str, elf: str) -> set[str]:
    return {
        match.group("name")
        for line in run(objdump, "-h", elf).splitlines()
        if (match := _SECTION_HEADER.match(line))
    }


def flash_literals_per_function(
    objdump: str, elf: str, sections: list[str]
) -> dict[str, int]:
    counts: dict[str, int] = {}
    args = ["-d"] + [f"--section={name}" for name in sections] + [elf]
    current: str | None = None
    for line in run(objdump, *args).splitlines():
        if header := _FUNC_HEADER.match(line):
            current = header.group("name")
            counts.setdefault(current, 0)
            continue
        if current is None:
            continue
        if hit := _L32R_VALUE.search(line):
            if FLASH_RODATA_LO <= int(hit.group("value"), 16) < FLASH_RODATA_HI:
                counts[current] += 1
    return counts


def render(counts: dict[str, int]) -> str:
    lines = [
        "# function, flash-pointing literals loaded from a RAM-resident text section",
        "# regenerate: just iram-flash-literals-esp32v3",
    ]
    lines += [f"{name}, {counts[name]}" for name in sorted(counts)]
    return "\n".join(lines) + "\n"


def parse_table(text: str) -> dict[str, int]:
    table: dict[str, int] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, count = line.rpartition(",")
        table[name.strip()] = int(count)
    return table


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf")
    parser.add_argument("--objdump", default="xtensa-esp32-elf-objdump")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--write-baseline", action="store_true")
    args = parser.parse_args()

    if shutil.which(args.objdump) is None:
        print(
            f"error: {args.objdump} is not on PATH — prepend the esp toolchain's\n"
            "       ~/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin",
            file=sys.stderr,
        )
        return 2

    present = section_headers(args.objdump, args.elf)
    sections = [name for name in RAM_TEXT_SECTIONS if name in present]
    if not sections:
        print(
            f"error: none of {', '.join(RAM_TEXT_SECTIONS)} are in {args.elf} — the\n"
            "       image has no RAM-resident text, which cannot be right",
            file=sys.stderr,
        )
        return 2

    counts = flash_literals_per_function(args.objdump, args.elf, sections)
    table = render(counts)

    if args.write_baseline:
        if args.baseline is None:
            print("error: --write-baseline needs --baseline <path>", file=sys.stderr)
            return 2
        args.baseline.write_text(table)
        print(f"wrote baseline: {args.baseline} ({len(counts)} functions)")
        return 0

    if args.baseline is None:
        print(table, end="")
        return 0

    old = parse_table(args.baseline.read_text())
    grew = [
        (name, old.get(name, 0), count)
        for name, count in sorted(counts.items())
        if count > old.get(name, 0)
    ]
    total = sum(counts.values())
    if grew:
        print(
            "FAIL: functions in RAM-resident text now load more flash literals\n"
            "      than the baseline — the ISR-in-RAM rule is at risk:",
            file=sys.stderr,
        )
        for name, before, after in grew:
            print(f"  {name}: {before} -> {after}", file=sys.stderr)
        return 1
    print(
        f"OK: no RAM-resident function gained a flash literal "
        f"({len(counts)} functions, {total} flash literals in total)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

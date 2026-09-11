#!/usr/bin/env python3
"""Classify an artefact's UNSUPPORTED decode sites as real code or literal pool.

Why this exists
---------------
`census.py` ranks the mnemonics `lp-xt-inst` cannot decode. Quoted on its own
that ranking is misleading, and on the ESP32-S3 it is misleading in a way that
would change a milestone's scope: 87 of the S3 image's 99 unsupported kinds
are PIE `ee.*` vector instructions, which reads as "this image needs a vector
unit". It does not. Xtensa literal pools sit *inside* `.text`, interleaved
with code, and objdump — reading the S3's PIE-bearing configuration — emits
plausible `ee.vmulas.*` mnemonics for constants.

The discriminator is the ELF symbol table. Real code lives inside some
`[sym, sym+size)` of a sized code symbol; an interleaved literal pool and the
padding between functions do not. So: for every site whose mnemonic is on the
census's unsupported list, is it inside a sized code symbol?

A site INSIDE a sized symbol is a genuine gap — an instruction the part
executes and the decoder cannot read. A site OUTSIDE one is a constant being
read as an instruction, and no amount of decoder work makes it go away.

This is the same test the classic's M0 report ran, run the same way, so the
two chips' numbers are comparable.

Licence note (AGENTS.md): objdump and nm are used here as tools whose *output*
is fact. No binutils source, table or logic is read or adapted.

Usage
-----
    scripts/emu/xtensa-inventory/in-symbol.py <artefact.elf> \\
        --census <census.json> [--objdump PATH] [--nm PATH] [--prefix ee.]

`--census` is the JSON `census.py --json` wrote for the same artefact; its
unsupported list is what this script looks for. `--prefix` singles out one
family for its own line (default `ee.`, the PIE question).
"""

from __future__ import annotations

import argparse
import bisect
import json
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

_NM_RE = re.compile(r"^([0-9a-f]+)(?: ([0-9a-f]+))? (\S) (.+)$")


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"in-symbol: failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}")
    return proc.stdout


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


def parse_line(line: str) -> tuple[int, str] | None:
    """`(addr, mnemonic)` for an objdump instruction line, the way
    `objdiff.rs::parse_line` splits one: address, tab, hex bytes, tab, text."""
    if ":\t" not in line:
        return None
    addr_part, rest = line.split(":", 1)
    addr_part = addr_part.strip()
    if not addr_part or any(c not in "0123456789abcdef" for c in addr_part):
        return None
    fields = [f for f in rest.split("\t") if f.strip()]
    if len(fields) < 2:
        return None
    return int(addr_part, 16), fields[1].split()[0].strip()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("artefact", type=Path)
    ap.add_argument("--census", type=Path, required=True)
    ap.add_argument("--objdump", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32s3-elf-objdump")
    ap.add_argument("--nm", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32s3-elf-nm")
    ap.add_argument("--prefix", default="ee.")
    ap.add_argument("--top", type=int, default=20)
    args = ap.parse_args()

    census = json.loads(args.census.read_text(encoding="utf-8"))
    raw = census.get("unsupported") or census.get("unsupported_mnemonics") or {}
    if isinstance(raw, list):
        raw = {k: v for k, v in raw}
    supported = census.get("supported") or {}
    if isinstance(supported, list):
        supported = {k: v for k, v in supported}

    # ⚠️ **Only PURE-unsupported mnemonics can be tested this way.** This
    # script finds sites by matching objdump's mnemonic text, and a mnemonic
    # that appears in BOTH the census's supported and unsupported lists cannot
    # be attributed site by site: `retw.n` is decoded 7,411 times and misread
    # once, and matching the name finds all 7,412. Including those would
    # report thousands of "genuine decoder gaps" that are nothing of the kind.
    # So the mixed mnemonics are excluded and NAMED, never silently dropped —
    # the classic's M0 report drew the same line and called the remainder
    # "pure-unsupported".
    mixed = sorted(set(raw) & set(supported))
    wanted = set(raw) - set(mixed)
    if not wanted:
        sys.exit(f"in-symbol: no unsupported list in {args.census}")

    syms = symbol_ranges(args.nm, args.artefact)
    starts = [s for s, _, _ in syms]

    inside: Counter = Counter()
    outside: Counter = Counter()
    inside_sites: list[tuple[int, str, str]] = []

    for line in run([str(args.objdump), "-d", str(args.artefact)]).splitlines():
        parsed = parse_line(line)
        if parsed is None:
            continue
        addr, mnemonic = parsed
        if mnemonic not in wanted:
            continue
        i = bisect.bisect_right(starts, addr) - 1
        if i >= 0 and syms[i][0] <= addr < syms[i][1]:
            inside[mnemonic] += 1
            inside_sites.append((addr, mnemonic, syms[i][2]))
        else:
            outside[mnemonic] += 1

    total = sum(inside.values()) + sum(outside.values())
    print(f"=== xtensa-inventory in-symbol test: {args.artefact} ===")
    print(f"objdump: {args.objdump}")
    print(f"census:  {args.census}")
    print(f"unsupported mnemonic kinds from the census: {len(raw)}")
    print(f"  pure-unsupported (tested here):  {len(wanted)} kinds, "
          f"{sum(raw[k] for k in wanted)} sites")
    print(f"  ALSO in the supported list (untestable by mnemonic match, "
          f"excluded): {len(mixed)} kinds, {sum(raw[k] for k in mixed)} sites")
    for k in mixed:
        print(f"      {k}: {raw[k]} unsupported site(s), {supported[k]} decoded")
    print(f"sized code symbols: {len(syms)}")
    print()
    print(f"unsupported sites located in the listing: {total}")
    print(f"  INSIDE  a sized code symbol: {sum(inside.values())}  "
          f"({len(inside)} kinds)  <- genuine decoder gaps")
    print(f"  OUTSIDE any sized symbol:    {sum(outside.values())}  "
          f"({len(outside)} kinds)  <- literal pool / inter-function padding")
    print()

    pi = sum(n for k, n in inside.items() if k.startswith(args.prefix))
    po = sum(n for k, n in outside.items() if k.startswith(args.prefix))
    print(f"`{args.prefix}*` family: {pi + po} sites — "
          f"{pi} inside a sized symbol, {po} outside")
    if pi == 0:
        print(f"  => NO `{args.prefix}*` instruction appears in this image's real, "
              f"symbol-bounded code.")
    print()

    if inside:
        print(f"--- unsupported mnemonics INSIDE a sized symbol (top {args.top}) ---")
        for mnemonic, n in inside.most_common(args.top):
            where = {s for a, m, s in inside_sites if m == mnemonic}
            sample = sorted(where)[:3]
            print(f"  {n:>6}  {mnemonic:<24} in {len(where)} symbol(s), e.g. {sample[0]}")
    else:
        print("--- NO unsupported mnemonic falls inside any sized code symbol ---")
    print()
    print(f"--- unsupported mnemonics OUTSIDE any symbol (top {args.top}) ---")
    for mnemonic, n in outside.most_common(args.top):
        print(f"  {n:>6}  {mnemonic}")


if __name__ == "__main__":
    main()

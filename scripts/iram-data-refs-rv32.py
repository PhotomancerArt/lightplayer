#!/usr/bin/env python3
"""Which lookup tables do RAM-resident functions read out of FLASH? (RV32 / ESP32-C6)

The C6 twin of `iram-flash-literals.py`, for the one question that matters when
a constant is moved out of `.data`: the ISR-in-RAM rule (memory note
`isr-path-in-ram-rule`) says nothing on an interrupt path may read flash, and
esp-hal's `place-switch-tables-in-ram` (see `lp-fw/fw-esp32c6/rwdata_hook.x`)
is exactly a decision about which compiler-made tables live in RAM.

For every function in the image's RAM-resident text sections (`.trap`,
`.rwtext`, `.rwtext.wifi`) it resolves the addresses the code forms with
`lui`/`auipc` + `addi`/load/store pairs and reports each reference that lands
on a `.Lswitch.table.*` symbol (LLVM's match lookup tables, the thing the flag
moves) whose address is in the flash-mapped window (0x4200_0000..0x4400_0000).
Exit status 1 when there is at least one: a RAM function would take a flash
read it did not take before.

    scripts/iram-data-refs-rv32.py <elf> [--objdump rust-objdump] [--nm rust-nm]
                                         [--list]    # print RAM-side refs too

What it does NOT cover, on purpose:

* `.rodata.cst*` pools (merged across every object, the Wi-Fi/BLE blobs' IRAM
  code reads them) — the hook keeps them in RAM wholesale, so there is nothing
  to check;
* panic `Location`s and `log` strings — `.Lanon.*` flash reads that the cold
  tails of RAM functions have always made (docs/debt/classic-iram-handlers-
  reach-flash.md has the same class on the classic);
* an indirect read through a flash-resident callee — the callee is flash code,
  the rule is already broken there and this check is not the one that finds it.

A reference is a lead, not a verdict, as with the classic's script: read the
function it names (`rust-objdump -d`) before deciding a table must stay.
"""

from __future__ import annotations

import argparse
import bisect
import re
import subprocess
import sys

RAM_SECTIONS = [".trap", ".rwtext", ".rwtext.wifi"]
FLASH_LO, FLASH_HI = 0x4200_0000, 0x4400_0000
RAM_LO, RAM_HI = 0x4080_0000, 0x4088_0000

RX_FN = re.compile(r"^([0-9a-f]{8}) <(.*)>:$")
RX_INS = re.compile(r"^\s*([0-9a-f]+):\s+(\w[\w.]*)\s*(.*)$")
RX_MEM = re.compile(r"(-?\w+)\((\w+)\)")
LOADS_STORES = {"lw", "lh", "lhu", "lb", "lbu", "sw", "sh", "sb", "flw", "fsw"}
NO_CLOBBER = {"sw", "sh", "sb", "fsw", "beq", "bne", "blt", "bge", "bltu", "bgeu", "j", "jr", "ret"}


def symbols(nm: str, elf: str) -> tuple[list[int], list[tuple[int, int, str]]]:
    out = subprocess.run([nm, "-n", "-S", "-C", elf], capture_output=True, text=True, check=True).stdout
    syms: list[tuple[int, int, str]] = []
    for line in out.splitlines():
        p = line.split(None, 3)
        if len(p) < 4:
            continue
        try:
            addr, size = int(p[0], 16), int(p[1], 16)
        except ValueError:
            continue
        syms.append((addr, size, p[3]))
    syms.sort()
    return [s[0] for s in syms], syms


def resolve(addrs: list[int], syms: list[tuple[int, int, str]], a: int) -> str | None:
    i = bisect.bisect_right(addrs, a) - 1
    while i >= 0 and syms[i][0] >= a - 4096:
        sa, ss, name = syms[i]
        if sa <= a < sa + max(ss, 1):
            return name
        i -= 1
    return None


def references(elf: str, objdump: str, nm: str):
    addrs, syms = symbols(nm, elf)
    cmd = [objdump, "-d", "--no-show-raw-insn"]
    for s in RAM_SECTIONS:
        cmd += ["-j", s]
    dis = subprocess.run(cmd + [elf], capture_output=True, text=True, check=True).stdout
    fn = None
    regs: dict[str, int] = {}
    seen: set[tuple[str, str]] = set()

    def hit(a: int):
        if fn is None:
            return None
        name = resolve(addrs, syms, a)
        if name and name.startswith(".Lswitch.table"):
            key = (fn, name)
            if key not in seen:
                seen.add(key)
                return (fn, name, a)
        return None

    found = []
    for line in dis.splitlines():
        m = RX_FN.match(line)
        if m:
            fn, regs = m.group(2), {}
            continue
        m = RX_INS.match(line)
        if not m or fn is None:
            continue
        pc, op = int(m.group(1), 16), m.group(2)
        rest = m.group(3).split("#")[0].strip()
        ops = [o.strip() for o in rest.split(",")] if rest else []
        try:
            if op == "auipc":
                regs[ops[0]] = (pc + (int(ops[1], 0) << 12)) & 0xFFFFFFFF
            elif op == "lui":
                regs[ops[0]] = (int(ops[1], 0) << 12) & 0xFFFFFFFF
            elif op == "addi" and ops[1] in regs:
                a = (regs[ops[1]] + int(ops[2], 0)) & 0xFFFFFFFF
                regs[ops[0]] = a
                if (r := hit(a)):
                    found.append(r)
            elif op in LOADS_STORES:
                mm = RX_MEM.match(ops[1])
                if mm and mm.group(2) in regs:
                    a = (regs[mm.group(2)] + int(mm.group(1), 0)) & 0xFFFFFFFF
                    if (r := hit(a)):
                        found.append(r)
                if op.startswith("l") and mm and ops[0] != mm.group(2):
                    regs.pop(ops[0], None)
            elif ops and ops[0] in regs and op not in NO_CLOBBER:
                regs.pop(ops[0], None)
        except (IndexError, ValueError):
            continue
    return found


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("elf")
    ap.add_argument("--objdump", default="rust-objdump")
    ap.add_argument("--nm", default="rust-nm")
    ap.add_argument("--list", action="store_true", help="also print the references that are in RAM")
    args = ap.parse_args()

    found = references(args.elf, args.objdump, args.nm)
    in_flash = [f for f in found if FLASH_LO <= f[2] < FLASH_HI]
    in_ram = [f for f in found if RAM_LO <= f[2] < RAM_HI]
    print(f"{len(found)} lookup-table reads by RAM-resident functions: "
          f"{len(in_ram)} from RAM, {len(in_flash)} from FLASH")
    if args.list:
        for fn, name, a in sorted(in_ram):
            print(f"  ram    {a:#010x} {name[:90]}  <- {fn[:90]}")
    for fn, name, a in sorted(in_flash):
        print(f"  FLASH  {a:#010x} {name[:90]}  <- {fn[:90]}")
    if in_flash:
        print("a RAM-resident function reads a lookup table that lives in flash: "
              "add it to lp-fw/fw-esp32c6/rwdata_hook.x", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

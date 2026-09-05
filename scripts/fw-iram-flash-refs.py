#!/usr/bin/env python3
"""Which RAM-resident firmware functions still reach into flash?

Scans every RAM text section of an Xtensa ELF (`.rwtext`, `.vectors`,
`.rtc_fast.text`), follows every `l32r` to its literal, and classifies the
literal's value by the section it points into. Literals that hold a flash
`.rodata` address (a table, a `core::panic::Location`, a format-spec piece)
or a flash `.text` address (a function pointer for `callx8`) are counted per
function, as are direct `call8`/`j` targets in flash `.text`.

This is the ISR-in-RAM rule's verification step (memory: isr-path-in-ram-rule;
docs/debt/classic-iram-handlers-reach-flash.md): `#[ram]` on a handler is a
hope, the literal pool is the fact. A hit is not automatically a defect — most
are panic tails that never run — so the `--dump` mode prints the annotated
disassembly of a function so the branch a hit sits on can be read.

usage:
  fw-iram-flash-refs.py <elf>                      # per-function summary
  fw-iram-flash-refs.py <elf> --dump rmt_isr ...   # annotated disassembly

The Xtensa binutils (`xtensa-esp-elf-objdump`, `xtensa-esp-elf-nm`) must be on
PATH — `~/.rustup/toolchains/esp/xtensa-esp-elf/*/xtensa-esp-elf/bin`. Tested
against fw-esp32v3 (`release-esp32v3` profile); the ESP32-S3 uses the same
section names and would need only the address classifier extended.
"""
import bisect
import re
import subprocess
import sys

OBJDUMP = "xtensa-esp-elf-objdump"
NM = "xtensa-esp-elf-nm"

RAM_TEXT_SECTIONS = [".rwtext", ".vectors", ".rwtext.wifi", ".rtc_fast.text"]
FLASH_RODATA_SECTIONS = {".flash.appdesc", ".rodata_merge", ".rodata", ".rodata.wifi"}
FLASH_TEXT_SECTIONS = {".text"}


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    elf = sys.argv[1]
    dump_filters = [a for a in sys.argv[2:] if a != "--dump"]

    # ---- sections: bytes + classification ranges --------------------------
    sections = []
    hdr = run(OBJDUMP, "-h", elf)
    for m in re.finditer(
        r"^\s+\d+\s+(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+.*?\n\s+(.*)$",
        hdr,
        re.M,
    ):
        name, size, vma, _lma, _off, flags = m.groups()
        size = int(size, 16)
        vma = int(vma, 16)
        if size == 0 or "CONTENTS" not in flags or "ALLOC" not in flags:
            continue
        sections.append((name, vma, size))

    mem = {}
    for name, vma, size in sections:
        out = run(OBJDUMP, "-s", "-j", name, elf)
        buf = bytearray(size)
        for line in out.splitlines():
            m = re.match(r"^ ([0-9a-f]+) ((?:[0-9a-f]{2,8} ?){1,4})", line)
            if not m:
                continue
            addr = int(m.group(1), 16)
            data = bytes.fromhex(m.group(2).replace(" ", ""))
            off = addr - vma
            buf[off : off + len(data)] = data
        mem[(vma, vma + size, name)] = buf

    def section_of(addr):
        for (s, e, n), _ in mem.items():
            if s <= addr < e:
                return n
        return None

    def read32(addr):
        for (s, e, _n), buf in mem.items():
            if s <= addr < e - 3:
                o = addr - s
                return int.from_bytes(buf[o : o + 4], "little")
        return None

    def read_bytes(addr, n):
        for (s, e, _n), buf in mem.items():
            if s <= addr < e:
                o = addr - s
                return bytes(buf[o : o + n])
        return b""

    def classify(val):
        sec = section_of(val)
        if sec in FLASH_RODATA_SECTIONS:
            return "RODATA_FLASH"
        if sec in FLASH_TEXT_SECTIONS:
            return "TEXT_FLASH"
        if sec in RAM_TEXT_SECTIONS:
            return "IRAM"
        if sec is not None:
            return sec
        if 0x40000000 <= val < 0x40070000:
            return "ROM"
        if 0x3FF00000 <= val < 0x3FF80000:
            return "PERIPH"
        return "IMM"

    # ---- symbols ----------------------------------------------------------
    syms = []
    for line in run(NM, "-C", "-n", elf).splitlines():
        parts = line.split(" ", 2)
        if len(parts) < 3 or not re.match(r"^[0-9a-f]{8}$", parts[0]):
            continue
        syms.append((int(parts[0], 16), parts[1], parts[2]))
    sym_addrs = [s[0] for s in syms]

    def sym_for(addr):
        i = bisect.bisect_right(sym_addrs, addr) - 1
        if i < 0:
            return "?"
        a, _t, n = syms[i]
        off = addr - a
        return f"{n}+0x{off:x}" if off else n

    def preview(addr):
        b = read_bytes(addr, 48)
        if b and all(32 <= c < 127 or c in (9, 10) for c in b[:16]):
            s = b.split(b"\0")[0][:44].decode("ascii", "replace")
            return f'"{s}"'
        return ""

    # ---- disassembly of the RAM text sections -----------------------------
    present = [s for s in RAM_TEXT_SECTIONS if any(n == s for n, _, _ in sections)]
    dis = run(OBJDUMP, "-d", "--no-show-raw-insn", *sum([["-j", s] for s in present], []), elf)
    funcs = {}
    order = []
    cur = None
    for line in dis.splitlines():
        m = re.match(r"^([0-9a-f]{8}) <(.+)>:$", line)
        if m:
            cur = m.group(2)
            if cur in funcs:
                cur = cur + "@" + m.group(1)
            funcs[cur] = []
            order.append(cur)
            continue
        m = re.match(r"^\s*([0-9a-f]+):\s+(.*)$", line)
        if m and cur:
            funcs[cur].append((int(m.group(1), 16), m.group(2).strip()))

    summary = []
    annotated = {}
    unique_values = set()
    for name in order:
        insns = funcs[name]
        if not insns:
            continue
        hits = []
        lines = []
        for addr, text in insns:
            mark = ""
            m = re.match(r"l32r\s+(a\d+),\s*([0-9a-f]+)", text)
            if m:
                lit = int(m.group(2), 16)
                val = read32(lit)
                if val is None:
                    mark = f"   ;; lit@{lit:08x} = ??"
                else:
                    cls = classify(val)
                    mark = f"   ;; = 0x{val:08x} {cls} {sym_for(val)} {preview(val)}"
                    if cls in ("RODATA_FLASH", "TEXT_FLASH"):
                        hits.append((addr, cls, val))
                    if cls == "RODATA_FLASH":
                        unique_values.add(val)
            else:
                m = re.match(r"(call8|call0|call4|call12|j)\s+([0-9a-f]+)", text)
                if m:
                    tgt = int(m.group(2), 16)
                    if classify(tgt) == "TEXT_FLASH":
                        mark = f"   ;; -> TEXT_FLASH {sym_for(tgt)}"
                        hits.append((addr, "CALL_FLASH", tgt))
            lines.append(f"{addr:08x}:  {text}{mark}")
        annotated[name] = lines
        if hits:
            nro = sum(1 for h in hits if h[1] == "RODATA_FLASH")
            ntx = sum(1 for h in hits if h[1] == "TEXT_FLASH")
            ncl = sum(1 for h in hits if h[1] == "CALL_FLASH")
            summary.append((name, nro, ntx, ncl, insns[0][0], len(insns)))

    if dump_filters:
        for name in order:
            if any(f in name for f in dump_filters):
                print(f"==== {name}")
                print("\n".join(annotated[name]))
                print()
        return

    print("rodata = l32r literals holding a flash .rodata address")
    print("&text  = l32r literals holding a flash .text address (callx8 targets)")
    print("call   = direct call8/j into flash .text")
    print("(`sym_sidata` is the literal pool decoded as code — ignore its row)")
    print()
    print(f"{'rodata':>6} {'&text':>5} {'call':>4}  {'addr':>8} {'insns':>5}  function")
    tot = [0, 0, 0]
    for name, nro, ntx, ncl, a, n in sorted(summary, key=lambda s: -s[1]):
        tot[0] += nro
        tot[1] += ntx
        tot[2] += ncl
        print(f"{nro:6d} {ntx:5d} {ncl:4d}  {a:08x} {n:5d}  {name}")
    print(f"{tot[0]:6d} {tot[1]:5d} {tot[2]:4d}  TOTAL  ({len(unique_values)} unique rodata values)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""What the linked layout says the main task's stack is, read off the ELF.

    scripts/heap-budget-stack-layout.py <elf>

Prints one JSON object:

    {"stackTop": <_stack_start>, "stackBottom": <_stack_end>,
     "stackTotal": <_stack_start - _stack_end>,
     "staticsEnd": <end of the highest allocated section below the stack>,
     "staticsSection": "<its name>", "gap": <stackBottom - staticsEnd>}

Why this exists: on every chip the heap-budget gate boots (C6, classic, S3),
esp-hal's `ld/sections/stack.x` opens `.stack` at `_stack_end = ABSOLUTE(.)`
right after `.data`/`.bss` and closes it at `ORIGIN(RWDATA) + LENGTH(RWDATA)`,
so the main stack is the RESIDUAL of RWDATA — eight new bytes of statics move
it by eight. Every `stack_probe.rs` reports `_stack_start - _stack_end` as the
`of <total> B` on its `[stack] heartbeat:` line. `heap-budget-check.sh` grades
that reported figure against this derived one instead of against a number
frozen in the record (docs/adr/2026-09-23-heap-budget-record-split-and-derived-stack.md):

- `stackTotal` here must EQUAL the heartbeat's — the probe reports the layout;
- `stackTop` must equal the record's — it is `ORIGIN + LENGTH` of RWDATA, a
  memory-map figure that genuinely only a linker-script change moves;
- `gap` must be under 4 (`.stack` is `ALIGN(4)`) — the stack is exactly what
  the statics leave, with nothing hidden between them.

Pure stdlib, reading the section header and symbol tables directly (32-bit
little-endian ELF — RV32 and Xtensa alike). Shape after
`scripts/emu/elf-section-digest.py`, and for its reason: `readelf`/`nm` are
not on a stock macOS and a host `llvm-nm` is not guaranteed on every runner.
"""

import json
import struct
import sys

SHT_SYMTAB = 2
SHT_NOBITS = 8
SHF_ALLOC = 0x2


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    with open(sys.argv[1], "rb") as f:
        blob = f.read()
    if blob[:4] != b"\x7fELF":
        raise SystemExit(f"{sys.argv[1]}: not an ELF")
    if blob[4] != 1 or blob[5] != 1:
        raise SystemExit(f"{sys.argv[1]}: only 32-bit little-endian ELF is supported")

    e_shoff, = struct.unpack_from("<I", blob, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", blob, 0x2E)
    heads = []
    for i in range(e_shnum):
        off = e_shoff + i * e_shentsize
        # name, type, flags, addr, offset, size, link
        heads.append(struct.unpack_from("<IIIIIII", blob, off))

    def cstr(table_off: int, idx: int) -> str:
        end = blob.index(b"\0", table_off + idx)
        return blob[table_off + idx : end].decode("utf-8", "replace")

    shstr_off = heads[e_shstrndx][4]
    names = [cstr(shstr_off, h[0]) for h in heads]

    wanted = {"_stack_start": None, "_stack_end": None}
    for h in heads:
        if h[1] != SHT_SYMTAB:
            continue
        _, _, _, _, sym_off, sym_size, link = h
        str_off = heads[link][4]
        for k in range(sym_size // 16):
            st_name, st_value = struct.unpack_from("<II", blob, sym_off + k * 16)
            if st_name == 0:
                continue
            name = cstr(str_off, st_name)
            if name in wanted and wanted[name] is None:
                wanted[name] = st_value
    missing = [n for n, v in wanted.items() if v is None]
    if missing:
        raise SystemExit(f"{sys.argv[1]}: no {', '.join(missing)} in the symbol table (stripped?)")
    top, bottom = wanted["_stack_start"], wanted["_stack_end"]

    # The highest allocated section that ends at or below the stack's bottom:
    # on every chip here that is `.bss` (or whatever NOLOAD section esp-hal
    # places last before `.stack`) — the statics the stack is the residual of.
    statics_end, statics_name = None, None
    for h, name in zip(heads, names):
        _, typ, flags, addr, _, size, _ = h
        if not flags & SHF_ALLOC or size == 0 or name == ".stack":
            continue
        end = addr + size
        if end <= bottom and (statics_end is None or end > statics_end):
            statics_end, statics_name = end, name
    if statics_end is None:
        raise SystemExit(f"{sys.argv[1]}: no allocated section below _stack_end 0x{bottom:08x}")

    print(json.dumps({
        "stackTop": top,
        "stackBottom": bottom,
        "stackTotal": top - bottom,
        "staticsEnd": statics_end,
        "staticsSection": statics_name,
        "gap": bottom - statics_end,
    }))


if __name__ == "__main__":
    main()

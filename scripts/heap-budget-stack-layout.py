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
little-endian ELF — RV32 and Xtensa alike) through `scripts/elf32.py`, the one
ELF reader this script shares with `scripts/ram-ledger.py`. Shape after
`scripts/emu/elf-section-digest.py`, and for its reason: `readelf`/`nm` are
not on a stock macOS and a host `llvm-nm` is not guaranteed on every runner.
"""

import json
import sys

from elf32 import Elf32, ElfError


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    try:
        elf = Elf32.open(sys.argv[1])
    except ElfError as e:
        raise SystemExit(str(e))

    wanted = elf.symbol_values(["_stack_start", "_stack_end"])
    missing = [n for n, v in wanted.items() if v is None]
    if missing:
        raise SystemExit(f"{sys.argv[1]}: no {', '.join(missing)} in the symbol table (stripped?)")
    top, bottom = wanted["_stack_start"], wanted["_stack_end"]

    # The highest allocated section that ends at or below the stack's bottom:
    # on every chip here that is `.bss` (or whatever NOLOAD section esp-hal
    # places last before `.stack`) — the statics the stack is the residual of.
    statics_end, statics_name = None, None
    for s in elf.sections:
        if not s.alloc or s.size == 0 or s.name == ".stack":
            continue
        if s.end <= bottom and (statics_end is None or s.end > statics_end):
            statics_end, statics_name = s.end, s.name
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

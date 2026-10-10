#!/usr/bin/env python3
"""What the linked layout says the main task's stack (and, where it is the
residual instead, the main heap region) is, read off the ELF.

    scripts/heap-budget-stack-layout.py <elf>

Prints one JSON object:

    {"layout": "stack-residual" | "heap-residual",
     "stackTop": <_stack_start>, "stackBottom": <_stack_end>,
     "stackTotal": <_stack_start - _stack_end>,
     "staticsEnd": <end of the highest allocated section below the residual>,
     "staticsSection": "<its name>", "gap": <residual start - staticsEnd>,
     # heap-residual only:
     "heapMainStart", "heapMainEnd", "heapMainBytes",
     "reclaimedStart", "reclaimedEnd", "reclaimedBytes", "contiguous"}

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

`heap-residual` is the other layout (`fw-esp32c6/build.rs`'s `patched_stack_x`,
RAM research E4): the main stack is a fixed span at the top of `dram2_seg`,
and what the statics leave of RWDATA is the main HEAP region instead, the
linker's `_heap_main_start .. _heap_main_end`, followed by the reclaimed tail
`_heap_reclaimed_start .. _heap_reclaimed_end`, which ends where the stack
begins. There the premise moves with the residual: `gap` (statics to
`_heap_main_start`, `.heap_main` is `ALIGN(8)`) must be under 8, and
`contiguous` (main → reclaimed → stack, end to start) must hold. The image
says which layout it is by carrying the `_heap_main_*` symbols or not.

Pure stdlib, reading the section header and symbol tables directly (32-bit
little-endian ELF — RV32 and Xtensa alike) through `scripts/elf32.py`, the one
ELF reader this script shares with `scripts/ram-ledger.py`. Shape after
`scripts/emu/elf-section-digest.py`, and for its reason: `readelf`/`nm` are
not on a stock macOS and a host `llvm-nm` is not guaranteed on every runner.
"""

import json
import sys

from elf32 import Elf32, ElfError

# Sections the residual itself is made of: never "the statics below it".
RESIDUAL_SECTIONS = {".stack", ".heap_main", ".heap_reclaimed", ".heap_reclaimed_dram2"}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    try:
        elf = Elf32.open(sys.argv[1])
    except ElfError as e:
        raise SystemExit(str(e))

    wanted = elf.symbol_values([
        "_stack_start", "_stack_end",
        "_heap_main_start", "_heap_main_end", "_heap_reclaimed_start", "_heap_reclaimed_end",
    ])
    missing = [n for n in ("_stack_start", "_stack_end") if wanted[n] is None]
    if missing:
        raise SystemExit(f"{sys.argv[1]}: no {', '.join(missing)} in the symbol table (stripped?)")
    top, bottom = wanted["_stack_start"], wanted["_stack_end"]

    heap_syms = ["_heap_main_start", "_heap_main_end", "_heap_reclaimed_start", "_heap_reclaimed_end"]
    present = [n for n in heap_syms if wanted[n] is not None]
    if present and len(present) != len(heap_syms):
        absent = sorted(set(heap_syms) - set(present))
        raise SystemExit(f"{sys.argv[1]}: a heap-residual layout missing {', '.join(absent)}")
    heap_residual = bool(present)
    residual_start = wanted["_heap_main_start"] if heap_residual else bottom

    # The highest allocated section that ends at or below where the residual
    # begins: on every chip here that is `.bss` (or whatever NOLOAD section
    # esp-hal places last before `.stack`) — the statics the residual is what
    # is left of.
    statics_end, statics_name = None, None
    for s in elf.sections:
        if not s.alloc or s.size == 0 or s.name in RESIDUAL_SECTIONS:
            continue
        if s.end <= residual_start and (statics_end is None or s.end > statics_end):
            statics_end, statics_name = s.end, s.name
    if statics_end is None:
        raise SystemExit(f"{sys.argv[1]}: no allocated section below 0x{residual_start:08x}")

    out = {
        "layout": "heap-residual" if heap_residual else "stack-residual",
        "stackTop": top,
        "stackBottom": bottom,
        "stackTotal": top - bottom,
        "staticsEnd": statics_end,
        "staticsSection": statics_name,
        "gap": residual_start - statics_end,
    }
    if heap_residual:
        ms, me = wanted["_heap_main_start"], wanted["_heap_main_end"]
        rs, re_ = wanted["_heap_reclaimed_start"], wanted["_heap_reclaimed_end"]
        out.update({
            "heapMainStart": ms,
            "heapMainEnd": me,
            "heapMainBytes": me - ms,
            "reclaimedStart": rs,
            "reclaimedEnd": re_,
            "reclaimedBytes": re_ - rs,
            "contiguous": me == rs and re_ == bottom,
        })
    print(json.dumps(out))


if __name__ == "__main__":
    main()

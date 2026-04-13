# lpvm-native-fa: Known Bugs

Status as of 2026-04-11. 19/27 filetests passing for `rv32fa.q32`.

## 1. Eviction spill ordering (spill handling)

When the backward walk processes a def for a vreg that was evicted from the
pool (the "dead value" path in `process_inst`), two things go wrong:

- The eviction `sw` is pushed to the backward list *before* the def's `li`,
  so after reversal the `li` runs first in forward order — overwriting the
  register before the eviction can save the old occupant.
- The code treats evicted-but-spilled vregs as truly dead ("no uses"), but
  they have a spill slot and their value is still needed. The def should
  write to the spill slot, not just to a temp register that gets freed.

**Filetests:** `spill_simple.glsl` (expected 351, actual 222),
`perf/spill-density.glsl` (1/2 passing).

**Root cause:** `walk.rs` `process_inst` lines 542-558.

## 2. RET_REGS overflow (>2 return words)

`emit_vinst` for `VInst::Ret` indexes into `RET_REGS[k]` without bounds
checking. `RET_REGS` has 2 entries (a0, a1). Functions returning vec4 or mat4
need sret (return via pointer), which isn't lowered yet.

**Filetests:** `native-call-vec4-return.glsl`, `native-call-mat4-return.glsl`,
`perf/mat4-reg-pressure.glsl`, `spill_pressure.glsl` — all panic with
"index out of bounds: the len is 2 but the index is 2".

**Root cause:** `walk.rs` line 1173, missing sret support.

## 3. Stack-passed arguments not implemented

Functions with >8 args need stack-passed parameters (both incoming and
outgoing). The allocator currently only handles register-passed args.

**Filetests:** `perf/stack-args-incoming.glsl` (0/3),
`perf/stack-args-incoming-16.glsl` (0/1),
`perf/stack-args-outgoing.glsl` (compile-fail, 0/2).

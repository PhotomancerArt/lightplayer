# Milestone 3: Empty-Function Overhead

## Goal

Reduce the fixed per-function overhead in the emitter. Currently even the
identity function emits 9 instructions vs cranelift's 2 (4.50x). Three
sources of waste: unconditional frame pointer, entry-move roundtrip, and
redundant branch to epilogue.

## Suggested plan name

`fa3-perf-m3`

## Scope

**In scope**:
- **Frame pointer omission**: Skip s0/fp save/restore and `addi s0, sp, N`
  when the function has no spill slots and no callee-saved registers in use.
  Changes in `rv32/emit.rs` (`emit_prologue`, `emit_epilogue`) and
  `abi/frame.rs` (`FrameLayout::compute`).
- **Redundant branch elimination**: The `Br L0` / `Label L0` at function end
  emits `j 4` to the next instruction. Either the emitter should detect and
  skip this, or a peephole pass should remove it. The existing `peephole.rs`
  already handles this pattern but is not wired into `compile_function`.
- **Entry-move pass-through**: When a function parameter is returned directly
  (copy v_param -> v_ret -> Ret), the allocator roundtrips through a pool
  register (a1 -> t4 -> a0). Investigate whether the allocator can detect
  this and coalesce the entry param directly to the ret register.

**Out of scope**:
- Regalloc changes (M1-M2).
- New VInst types or lowering changes (M4-M5).

## Key decisions

- Frame pointer omission must check that nothing in the function relies on fp.
  The callee-saved detection in `used_callee_saved_from_output` already knows
  which s-regs are used; if none are and spill_slots == 0, the frame can be
  omitted entirely.

- Wiring `peephole.rs` into `compile_function` may have broader effects.
  Alternative: handle the specific `j +4` pattern in the emitter itself.

- The entry-move pass-through optimization touches the boundary between
  regalloc and emit. May be simplest to detect in `finish()` when generating
  entry edits.

## Deliverables

- Conditional frame pointer in `emit_prologue` / `emit_epilogue`.
- Eliminated redundant `j` to next instruction.
- Before/after on `callee_identity` and the perf suite.

## Dependencies

M1 and M2 should be completed first — they change register assignments which
affects whether fp omission triggers and what the baseline measurements are.

## Estimated scope

Moderate. ~50-100 lines across `emit.rs`, `rv32/emit.rs`, `abi/frame.rs`,
and possibly `compile.rs` (wiring peephole).

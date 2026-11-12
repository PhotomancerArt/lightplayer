# Milestone 1: Allocator Core

## Goal

Replace the stubbed `fa_alloc` backward walk with real register allocation that
produces `Vec<PInst>` for straight-line code. Unit-tested independently of the
compile pipeline.

## Suggested Plan Name

`fastalloc3-m1`

## Scope

### In scope

- **RegPool**: physical register pool with free-list and LRU eviction
- **SpillAlloc**: spill slot assignment and tracking
- **ConstPool**: IConst32 value tracking for rematerialization
- **Backward walk producing PInst**: replace `walk_region_stub` with a walk that
  makes real allocation decisions and emits PInst instructions
- **Liveness integration**: use `analyze_liveness` results to seed the walk's
  initial live set
- **Trace with real decisions**: trace entries record actual register
  assignments, spills, reloads (not "STUB")
- **Unit tests**: allocation of simple instruction sequences (iconst + arith +
  ret), spill under register pressure, reload from spill, IConst32
  rematerialization

### Out of scope

- Control flow (IfThenElse, Loop) — stubbed/error for now
- Call handling — stubbed/error for now
- Integration with `compile_function` — M2
- Filetest validation — M2

## Key Decisions

- `fa_alloc` outputs `Vec<PInst>` (existing type), not a new instruction type.
  This reuses the existing emitter without changes.
- The backward walk processes `Region::Linear` blocks. `IfThenElse`, `Loop`, and
  `Seq` produce errors in M1.
- Spill slots use frame-pointer-relative addressing, matching the existing
  `rv32::alloc` convention.
- LRU eviction: when all allocatable registers are occupied, evict the least
  recently used. The evicted vreg gets a spill slot and a store is emitted.

## Deliverables

- `fa_alloc/walk.rs` — real allocation logic replacing stubs
- `fa_alloc/spill.rs` — new module for spill slot management
- `fa_alloc/mod.rs` — updated entry point returning `Vec<PInst>` + `AllocTrace`
- Updated `fa_alloc/liveness.rs` — confirmed working for Linear regions
- Unit tests covering: simple allocation, register reuse, spill/reload, IConst32
  rematerialization, trace output correctness

## Dependencies

- M0-M4 from FastAlloc v2 (completed): VInst IR, lowering, region tree,
  peephole, PInst model, emitter, trace system, backward walk shell

## Estimated Scope

~400-600 lines of new/modified code in `fa_alloc/`. ~200 lines of tests.

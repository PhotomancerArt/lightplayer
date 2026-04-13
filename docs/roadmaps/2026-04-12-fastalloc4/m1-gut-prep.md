# Milestone 1: Gut + Prep

## Goal

Delete the broken direct-emission allocator and PInst layer. Define the new
AllocOutput types. Port the old `lpvm-native` forward emitter. Leave stubs for
the actual allocator. The crate compiles but allocation returns an error.

## Suggested Plan Name

`fastalloc4-m1`

## Scope

### In scope

- **Delete broken code:**
  - `fa_alloc/walk.rs` (entire file)
  - `rv32/inst.rs` (PInst type)
  - `rv32/rv32_emit.rs` (PInst → bytes emitter)
  - `rv32/debug/pinst.rs` (PInst debug formatting)
  - Remove PInst references from `rv32/mod.rs`, `emit.rs`, `fa_alloc/mod.rs`

- **Define new types in `fa_alloc/`:**
  - `Alloc` enum (Reg, Stack, None)
  - `AllocOutput` struct (allocs, inst_alloc_offsets, edits, spill slots, trace)
  - `EditPoint` enum (Before, After)
  - `Edit` enum (Move)
  - Stub `allocate()` returning `Err(AllocError::NotImplemented)`

- **Port forward emitter from `lpvm-native`:**
  - `EmitContext` from `lpvm-native/isa/rv32/emit.rs` → new `rv32/emit.rs`
  - Adapt for FA VInst types (VReg u16, VRegSlice, SymbolId + ModuleSymbols)
  - Adapt for AllocOutput (per-operand lookup instead of global vreg→preg)
  - Keep `use_vreg`/`def_vreg`/`store_def_vreg` pattern
  - Keep branch fixup, label offset, call emission, sret emission
  - Keep `encode_*` functions from `rv32/encode.rs` (unchanged)

- **Update orchestration:**
  - `emit.rs` calls new allocator + new emitter
  - `compile.rs` unchanged (calls `emit.rs`)

- **Compile check:** `cargo check -p lpvm-native-fa` passes. Tests may fail.

### Out of scope

- Actual register allocation (M2)
- Tests passing (allocator is stubbed)
- Filetests passing

## Key Decisions

- The emitter port adapts the old code to the FA crate's types rather than
  creating an abstraction layer. Direct adaptation is simpler and avoids
  unnecessary indirection.
- `AllocOutput` follows regalloc2's `Output` structure closely: flat alloc
  array indexed by instruction, sorted edit list.
- The `Alloc` type is simpler than regalloc2's (no reg class, no bit-packing)
  since we only have integer registers on RV32.

## Deliverables

- `fa_alloc/walk.rs` deleted
- `rv32/inst.rs` deleted (PInst)
- `rv32/rv32_emit.rs` deleted
- `rv32/debug/pinst.rs` deleted
- New `fa_alloc/mod.rs` with AllocOutput types and stub allocator
- New `rv32/emit.rs` ported from `lpvm-native`
- Updated `emit.rs` orchestration
- `cargo check -p lpvm-native-fa` passes

## Dependencies

- None (starting point)

## Estimated Scope

~500 lines deleted, ~1200 lines ported/adapted (emitter), ~100 lines new
(types + stubs).

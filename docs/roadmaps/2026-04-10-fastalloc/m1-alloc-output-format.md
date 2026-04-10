# Milestone 1: New Allocation Output Format + Emitter Edit Splicing

## Goal

Define the new `FastAllocation` output type that supports per-instruction
operand assignments and explicit move edits. Adapt the greedy allocator to
produce this format. Refactor the emitter to consume edit lists instead of a
static `vreg_to_phys` map. No new allocation algorithm — this milestone proves
the plumbing end-to-end.

## Suggested plan name

`fastalloc-m1`

## Scope

### In scope

- New `FastAllocation` struct with per-operand `PhysReg` assignments and
  `Vec<(EditPos, Edit)>` move edits
- `EditPos` enum: `Before(usize)`, `After(usize)`
- `Edit` enum: `Move { from: Location, to: Location }` where `Location` is
  `Reg(PhysReg)` or `Stack(u32)` (spill slot)
- Adapter that converts the existing `Allocation` (static map) into a
  `FastAllocation` (per-operand assignments, empty edit list, call-save edits
  generated from the old `regs_saved_for_call` logic)
- Emitter refactored to walk `FastAllocation.edits` and splice moves at
  `Before`/`After` positions
- Emitter reads per-operand PhysReg from `FastAllocation.operand_allocs`
  instead of calling `use_vreg`/`def_vreg` with the global map
- Config flag to switch between old emitter path (static map) and new emitter
  path (edit list) for validation
- All existing filetests pass with the new emitter path

### Out of scope

- Backward-walk algorithm (M2)
- Block splitting, control flow (M3)
- Performance improvements (the adapter produces identical code to the old path)
- Removing old allocator code

## Key Decisions

- The adapter from `Allocation` → `FastAllocation` generates the same call-save
  `sw`/`lw` sequences as the current emitter, so output is bit-identical. This
  proves the new format and emitter work correctly before introducing a new
  algorithm.
- `operand_allocs` is indexed by a flat `(inst_idx, operand_slot)` scheme.
  Each VInst's operands (uses then defs) get consecutive slots.

## Deliverables

- `regalloc/mod.rs`: `FastAllocation`, `EditPos`, `Edit`, `Location` types
- `regalloc/mod.rs` or `regalloc/adapt.rs`: `Allocation` → `FastAllocation`
  adapter
- `isa/rv32/emit.rs`: new emitter entry point consuming `FastAllocation`
- `config.rs`: `USE_FAST_ALLOC_EMIT: bool` flag
- All existing filetests pass with the new path

## Dependencies

None — this is the first milestone.

## Estimated Scope

~300-400 lines of new/changed code. The bulk is the emitter refactor (replacing
`use_vreg`/`def_vreg`/`store_def_vreg` with edit-list consumption) and the
adapter that converts the old `Allocation` format.

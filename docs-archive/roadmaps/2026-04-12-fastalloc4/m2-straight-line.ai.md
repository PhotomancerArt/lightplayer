# Milestone 2: Straight-Line Allocator

## Goal

Implement the backward walk allocator for Linear regions, producing per-operand
allocations and an edit list. Unit tested with snapshot tests. Filetests pass
for straight-line functions (no calls, no control flow).

## Suggested Plan Name

`fastalloc4-m2`

## Scope

### In scope

- **RegPool** (`fa_alloc/pool.rs`): LRU register pool with alloc, free, evict.
  Extracted from old walk.rs but cleaned up.

- **Backward walk** (`fa_alloc/walk.rs`): Walk Linear regions in reverse,
  filling in per-operand allocations and recording edits:
  - For each instruction (backward): resolve uses (allocate regs), process
    defs (free regs), record allocations in the alloc table
  - When evicting: record `Edit::Move` (reg → spill slot) at the appropriate
    program point
  - When reloading a spilled vreg: record `Edit::Move` (spill slot → reg)

- **Snapshot test infrastructure:**
  - `fa_alloc/render.rs`: `render_alloc_output(vinsts, pool, output) -> String`
    — renders AllocOutput as human-readable text with edits interleaved
    before/after each VInst, register assignments annotated. Used by both
    tests and CLI (`shader-rv32fa --show-alloc`).
  - `fa_alloc/mod.rs` `mod tests`: `expect_alloc(input, expected)` helper —
    parses VInst text, runs allocator, calls render, compares strings.
    Prints actual output on mismatch for easy blessing.

- **Unit tests using VInst text parser:**
  - Simple: iconst + ret (single vreg, no spill)
  - Binary: iconst + iconst + add + ret (multiple vregs)
  - Reuse: value used twice
  - Spill: more live vregs than allocatable registers
  - Dead value: def with no use
  - Multiple defs: sequential overwrites

- **Filetest validation:** straight-line filetests pass (those without calls
  or control flow). `spill_simple.glsl` is the key target.

### Out of scope

- Calls (M3)
- Sret (M3)
- IfThenElse / Loop (M4)
- Filetests requiring calls or control flow

## Key Decisions

- The backward walk fills `allocs[(inst_idx, operand_idx)]` directly. No
  intermediate data structure. Edits are appended to a vec and sorted at the
  end (or maintained sorted during the walk, since backward → reversed at end).

- Operand ordering follows VInst convention: defs first, then uses. The
  `for_each_def` / `for_each_use` iterators on VInst define the operand order.

- Entry parameters (precolored vregs) are handled as edits at the function
  entry: `Edit::Move` from ABI register to allocated register. This replaces
  the old "precolor bridge" logic.

## Deliverables

- `fa_alloc/pool.rs` — RegPool with LRU eviction
- `fa_alloc/walk.rs` — backward walk for Linear regions
- `fa_alloc/render.rs` — human-readable AllocOutput rendering
- `fa_alloc/mod.rs` — `allocate()` calls walk, returns AllocOutput; snapshot
  tests in `mod tests`
- Snapshot test infrastructure and unit tests
- Straight-line filetests passing under `rv32fa`

## Dependencies

- M1 (gut + prep): types defined, emitter ported, crate compiles

## Estimated Scope

~400-600 lines allocator, ~200 lines test infrastructure, ~200 lines tests.

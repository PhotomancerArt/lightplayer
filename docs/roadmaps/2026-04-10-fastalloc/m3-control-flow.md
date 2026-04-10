# Milestone 3: Control Flow Support

## Goal

Extend the fastalloc allocator to handle basic blocks, branches, if/else, and
loops. After this milestone, all existing shaders compile and execute correctly
with fastalloc.

## Suggested plan name

`fastalloc-m3`

## Scope

### In scope

- Block splitting: scan VInsts for `Label`, `Br`, `BrIf` to identify basic
  block boundaries
- Per-block backward walk with liveness reconciliation at block boundaries
- Structured control flow handling:
  - **Straight / fall-through**: successor's live-at-entry seeds our
    live-at-exit
  - **If/else merge**: live set is union of both branches' live-at-exit
  - **Loop header**: loop-carried values span the entire loop (reuse
    `LoopRegion` info from the lowerer)
- At block boundaries, all live vregs are reconciled to spill slots (safe
  canonical location)
- All existing filetests pass (including those with control flow)
- All emulator round-trip tests pass (`fw-tests`)

### Out of scope

- Performance optimization beyond correctness (M4)
- Removing old allocators (M4)
- Param-to-callee-saved optimization (future)

## Key Decisions

- **Block structure** derived from VInsts without building a full CFG:
  ```
  struct Block { start: usize, end: usize, kind: BlockKind }
  enum BlockKind { Straight, LoopHeader, LoopExit, IfThen, IfElse, Merge }
  ```
  Derivable from existing `Label` / `Br` / `BrIf` / `LoopRegion` info.

- **Processing order**: blocks processed in reverse emission order (reverse
  post-order for structured code). Liveness propagated backward.

- **Boundary rule**: at the start of each block (after backward processing),
  any vreg still marked live is moved to its spill slot. At the end of each
  block (before backward processing), seed the live set from successor
  expectations. This is conservative but correct — values cross block
  boundaries via stack, not registers.

- **Loop-carried values**: reuse `LoopRegion { header_idx, backedge_idx }`
  from the lowerer (same info used by linear scan's `extend_for_loops`).
  Values live across loop backedges must span the entire loop body.

## Deliverables

- `regalloc/fastalloc.rs`: block splitting + per-block allocation (~150 lines
  added)
- New filetests for control-flow patterns: if/else with live values across
  branches, loops with carried values, nested loops
- All existing filetests pass
- All `fw-tests` emulator tests pass

## Dependencies

- M2 (backward-walk allocator for straight-line code)

## Estimated Scope

~200-250 lines of new/changed code. The block splitting is ~50 lines; the
boundary liveness logic is ~100 lines; filetests ~50 lines.

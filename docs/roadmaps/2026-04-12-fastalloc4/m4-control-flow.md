# Milestone 4: Control Flow

## Goal

Extend the allocator to handle IfThenElse and Loop regions with
spill-at-boundary semantics expressed as edits. All filetests pass under
`rv32fa`.

## Suggested Plan Name

`fastalloc4-m4`

## Scope

### In scope

- **IfThenElse allocation:**
  - At merge point: record current register state, emit boundary spill edits
  - Walk else body: seed pool, walk backward, emit boundary edits
  - Walk then body: seed pool, walk backward, emit boundary edits
  - Walk head: walk backward, emit head exit boundary edits
  - All boundary transitions are `Edit::Move` (reg ↔ slot)

- **Loop allocation:**
  - Compute loop-live set from liveness analysis (union of header + body
    live_in + post-loop live)
  - At loop exit: boundary spill edits
  - Walk body backward, boundary spill/reload at body entry/exit
  - Walk header backward, boundary spill/reload at header entry/exit
  - Back-edge: boundary spill edits
  - Pre-loop entry: boundary spill edits to flow into loop slot convention

- **Seq region:** walk children in reverse, threading register state

- **Liveness integration:** use `analyze_liveness` (already working for
  IfThenElse and Loop) to determine live sets at boundaries

- **Unit tests (snapshot style):**
  - Simple if/else: branch, different values in each arm, merge
  - If without else
  - Nested if/else
  - Simple loop: counter, accumulator, exit condition
  - Loop with call inside
  - Values live across loop boundary (loop-carried)

- **Filetest validation:** ALL filetests pass:
  - `native-call-control-flow.glsl`
  - `perf/caller-save-pressure.glsl`
  - `perf/live-range-interference.glsl`
  - `perf/nested-call-overhead.glsl`
  - `perf/spill-density.glsl`
  - All remaining perf tests
  - `spill_simple.glsl`
  - `spill_pressure.glsl`

### Out of scope

- Removing old crate (M5)
- Allocation quality optimization (heuristics, hints)

## Key Decisions

- Spill-at-boundary is the same strategy as fastalloc3, but expressed as
  edits rather than direct PInst emission. At each boundary (if/else entry,
  loop entry/exit), all live values in registers get
  `Edit::Move(reg → slot)` and re-entering regions get
  `Edit::Move(slot → reg)`.

- This is the main novelty vs regalloc2: they handle merge points via
  parallel moves between blocks. We handle them via spill-all-at-boundary,
  which is simpler (no parallel move resolution needed) at the cost of more
  memory traffic. For shader code this is acceptable.

- The allocator walks the region tree recursively (same structure as
  fastalloc3's walk_ite/walk_loop) but records edits tagged with VInst
  indices instead of pushing PInsts.

## Deliverables

- Updated `fa_alloc/walk.rs` — IfThenElse, Loop, Seq region handling
- Updated `fa_alloc/liveness.rs` — verified working (likely no changes needed)
- Snapshot unit tests for control flow patterns
- ALL filetests passing under `rv32fa` (27/27)

## Dependencies

- M3 (calls + sret): call handling works, most filetests pass

## Estimated Scope

~400-600 lines allocator additions, ~200 lines tests. This is the most
complex milestone — region boundary semantics interact with the edit list
in non-obvious ways.

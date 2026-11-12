# fastalloc3-m3-revision Planning Notes

## Scope

Rewrite the IfThenElse and Loop handling in `fa_alloc` using the regalloc2
fastalloc "spill-at-boundary" convention instead of the reconciliation approach
from the original M3 plan. Also handle sret calls and param precoloring.

Goal: all existing filetests pass under `rv32fa`.

## Current State

### What works (M1 + M2 + old M3 phases 1-2)

- Backward walk for `Linear` and `Seq` regions
- LRU register pool with spill/reload
- Trace system for debugging
- PInst emission for all arithmetic, comparison, mov, load/store, ret
- BrIf, Br, Label → PInst::Beq/Bne/J/Label with label fixup system
- Select32 → PInst sequence (neg + sub + and + add)
- Direct calls (non-sret): clobber spill/reload, arg/ret placement
- `rv32fa` filetest target wired in
- CLI `shader-rv32fa` pipeline works for straight-line + call functions

### What doesn't work

- `walk_region` returns `Err(UnsupportedControlFlow)` for IfThenElse/Loop
- `process_call` returns `Err(UnsupportedSret)` for sret calls
- Param precoloring unused (params not placed in ARG_REGS at function entry)
- Labels emitted by lowering between regions are never walked (orphaned VInsts)

### Key architectural observation

The lowering (`lower.rs`) places BrIf, Br, and Label VInsts in specific
positions relative to IfThenElse regions:

- **head** = Linear containing just the BrIf
- **then_body** = computation only (no Br at end)
- **else_body** = Seq([br_region, else_computation]) where br_region = the
  `Br(end_label)` that jumps from then-end to merge
- **Label(else_label)** = in VInst stream but NOT inside any region
- **Label(end_label/merge)** = in VInst stream but NOT inside any region

This means Labels are "orphaned" — they exist in the flat VInst stream but no
region covers them. The IfThenElse walker must emit PInst::Label explicitly.

Also, the `Br(end_label)` that logically belongs to then (it's the jump at
the end of the then-path) is placed as the first child of else_body's Seq.

## Key Design Decision: Spill-at-Boundary

Adopt regalloc2's fastalloc invariant: **at every IfThenElse/Loop region
boundary, all live values are in their spill slots.**

This eliminates:
- Reconciliation between branches
- "Canonical branch" concept
- Back-edge fixup moves for loops
- Complex nested-if state tracking

The cost: more Sw/Lw pairs than strictly necessary. Acceptable for GLSL
shaders (short functions, moderate register pressure, L1-cached stack).

Within Linear/Seq regions, the allocator runs freely with LRU as it does now.

## Questions

### Q1: Refactor lowering or work around current structure?

**Context**: The current lowering puts `Br(end_label)` inside else_body's Seq
and leaves Labels between regions. This makes the IfThenElse walker's push
ordering awkward — exit spills need to appear before `Br` in forward order, but
the `Br` is walked as part of else_body.

**Option A (refactor lowering)**: Add `else_label` and `merge_label` fields to
`Region::IfThenElse`. Move `Br` out of else_body. Body regions contain only
computation. The walker emits Labels/J/BrIf at exact push positions.

**Option B (work around)**: The walker decomposes else_body's Seq, skipping
br_region and walking only the computation children. More fragile but avoids
touching the lowering.

**Option C (walker emits all control flow)**: The walker reads label IDs from
the VInst stream (BrIf target, Br target), emits all control-flow PInsts
itself, and delegates only computation to walk_region. The head/else_body
regions are walked but BrIf/Br/Label VInsts inside them are skipped (no-op in
process_inst when in IfThenElse context).

**Suggestion**: Option A. It's more work upfront but produces a clean separation
where body regions are pure computation and the walker owns all control flow.
This pays off for Loop too.

**Answer**: A. Refactor lowering. Add label fields to Region::IfThenElse and
Region::Loop. Body regions = pure computation. Walker emits all control flow.

### Q2: How should the IfThenElse walker handle the head?

**Context**: The head region contains the BrIf, which computes a branch
condition. In some cases the head is just BrIf (condition computed earlier). In
other cases there could be computation in the head (e.g. comparison instruction
+ BrIf in a Seq).

**Option A**: Head stays as a region. The walker walks it normally. BrIf is
processed by process_inst as it is now (emitting Beq/Bne). The walker just
needs to emit boundary spills between the head walk and the branch body walks.

**Option B**: Head computation is walked, but BrIf is extracted and emitted by
the walker (so the walker controls its exact push position).

**Suggestion**: Option A — keep it simple. The head is walked normally. The BrIf
emits its PInst during the walk. The walker emits boundary spills after walking
the head. The push ordering works because the head is walked last (after branch
bodies), so boundary spills pushed between head and branches end up in the right
forward position.

**Answer**: A. Head walked normally including BrIf. Walker emits boundary spills
between head walk and branch body walks.

### Q3: Loop walk strategy?

**Context**: The Loop region has `header` and `body`. With spill-at-boundary:
- Post-loop: flush to spill slots
- Walk body from clean pool
- Walk header from clean pool
- No back-edge fixup needed (both sides use spill slots)

The current lowering for loops:
- Entry `Br(header_label)` in its own region
- `Label(header_label)` in header
- Body from `lower_range`
- `Label(continuing)` + continuing block
- Back-edge `Br(header_label)`
- `Label(exit)`

**Question**: Should Loop use the same refactoring approach as IfThenElse
(add label fields to Region::Loop, walker emits control flow PInsts)?

**Suggestion**: Yes. Add `header_label`, `exit_label` to Region::Loop. The
walker emits Labels and the back-edge J. Body regions are pure computation.

**Answer**: Yes. Same refactoring approach as IfThenElse. Add label fields to
Region::Loop, walker emits all control flow.

### Q4: Param precoloring — when in the plan?

**Context**: Function parameters arrive in ARG_REGS (a0-a7). The backward walk
processes function entry last. At that point, param vregs should be in their
ARG_REG. Currently this is not wired up.

**Suggestion**: Handle as a late phase. After the backward walk completes and
pinsts are reversed, check if param vregs ended up in their ARG_REGs. If not,
emit Mv fixups at function entry. This is independent of the control flow work.

**Answer**: Pre-seed the pool with params in their ARG_REGs before starting
the backward walk (pool.alloc_fixed for each precolor). ~5 lines of code in
allocate(). The backward walk naturally finds params in the right registers.
If evicted by pressure, normal spill/reload handles it. Implement as a late
phase since it's independent of control flow.

### Q5: Sret calls — when in the plan?

**Context**: Callees returning >2 scalars use sret (struct return). The caller
passes an sret buffer pointer in a0, shifting other args to a1+. After the
call, results are loaded from the sret buffer.

**Suggestion**: Handle as a late phase after IfThenElse and Loop work. It's
orthogonal to the spill-at-boundary architecture — it's just extending the
existing process_call with sret buffer handling.

**Answer**: Late phase, after control flow works. Self-contained extension of
process_call.

## Notes

- The original M3 plan's phases 1-2 (branches/select + direct calls) are fully
  implemented and tested. No code needs to be reverted.
- The original M3 plan's phase 3 (IfThenElse with reconciliation) was never
  implemented — just `Err(UnsupportedControlFlow)`.
- regalloc2's fastalloc uses `reload_at_begin` per block + `process_branch` at
  branches. Our adaptation maps "block boundary" to "IfThenElse/Loop region
  boundary."
- The PInst Vec approach (no edit list) works if the IfThenElse walker controls
  the push ordering of Labels, J, BrIf, and boundary Sw/Lw.

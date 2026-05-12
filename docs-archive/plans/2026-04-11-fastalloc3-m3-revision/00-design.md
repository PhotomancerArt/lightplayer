# fastalloc3-m3-revision Design

## Scope

Rewrite IfThenElse and Loop handling in `fa_alloc` using regalloc2's fastalloc
"spill-at-boundary" convention. Add sret call support and param precoloring.
All existing filetests pass under `rv32fa`.

## File Structure

```
lp-shader/lpvm-native/src/
├── region.rs                  # UPDATE: add label fields to IfThenElse/Loop
├── lower.rs                   # UPDATE: store labels in regions, body regions = pure computation
├── fa_alloc/
│   ├── mod.rs                 # UPDATE: param precoloring (pre-seed pool)
│   ├── walk.rs                # UPDATE: walk_ite, walk_loop, boundary helpers
│   ├── spill.rs               # (no changes)
│   ├── liveness.rs            # UPDATE: IfThenElse/Loop liveness
│   └── trace.rs               # (no changes)
├── rv32/
│   ├── inst.rs                # (no changes)
│   └── rv32_emit.rs           # (no changes)
├── abi/
│   └── func_abi.rs            # UPDATE: sret buffer plumbing
├── compile.rs                 # UPDATE: pass sret info
└── emit.rs                    # UPDATE: pass sret info
```

## Architecture

```
                    allocate(lowered, func_abi)
                             │
                    ┌────────┴────────┐
                    │  pre-seed pool  │  ← param vregs → ARG_REGs
                    └────────┬────────┘
                             │
                    ┌────────┴────────┐
                    │  walk_region()  │  ← recursive on RegionTree
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
         Linear          walk_ite       walk_loop
       (existing)          (new)          (new)
              │              │              │
              ▼              ▼              ▼
        process_inst    1. flush pool   1. flush pool
        for each inst      to slots       to slots
        in reverse      2. walk else    2. walk body
                           + boundary      + boundary
                        3. walk then    3. walk header
                           + boundary      + boundary
                        4. walk head    4. emit labels
                        5. emit labels     + back-edge J
                           + J + BrIf
              │
              ├── Call  → clobber spill + arg moves + call + reloads
              ├── Call (sret) → sret buffer + shifted args + call + loads
              └── (existing arithmetic/cmp/mov/load/store/ret)
```

## Key Design Decisions

### Spill-at-Boundary Invariant

At every IfThenElse/Loop region boundary, all live values are in their spill
slots. This eliminates reconciliation between branches, "canonical branch"
selection, and back-edge fixup moves. Each branch is allocated independently
from a clean pool.

Within Linear/Seq regions, the allocator runs freely with LRU as it does now.

Cost: more Sw/Lw pairs than strictly necessary. Acceptable for GLSL shaders
(short functions, moderate register pressure, L1-cached stack frame).

### Walker Owns Control Flow

Body regions (then_body, else_body, loop body) contain only computation VInsts.
The IfThenElse and Loop walkers emit all control-flow PInsts (Labels, J,
Beq/Bne) and all boundary spills/reloads at exact positions in the backward
push sequence.

This requires refactoring the lowering to:
- Add `else_label`, `merge_label` fields to `Region::IfThenElse`
- Add `header_label`, `exit_label` fields to `Region::Loop`
- Remove Br/Label VInsts from body regions

### Boundary Helpers

Three helpers on WalkState manage the spill-at-boundary transitions:

- `flush_to_slots()` — for each occupied (preg, vreg): assign spill slot,
  emit Lw (reload in forward order), clear pool. Returns saved state.
- `seed_pool()` — repopulate pool from a saved state (vreg→preg assignments).
- `emit_exit_spills()` — for each occupied (preg, vreg): emit Sw (store in
  forward order). Pool is NOT cleared (backward walk needs vregs registered
  so defs can free them).

### Param Precoloring

Pre-seed the pool with param vregs in their ARG_REGs before starting the
backward walk (`pool.alloc_fixed(arg_reg, param_vreg)` for each precolor).
The backward walk naturally finds params at the correct registers. If evicted
by register pressure, normal spill/reload handles it.

### IfThenElse Backward Push Ordering

The walker pushes in this order (last pushed = first in forward after reversal):

```
Push 0: merge Lw reloads        → Forward: at merge point (last)
Push 1: else exit Sw spills     → Forward: at else exit
Push 2: else body PInsts        → Forward: else computation
Push 3: else entry Lw reloads   → Forward: at else entry
Push 4: PInst::Label(else)      → Forward: else label
Push 5: PInst::J(merge)         → Forward: end of then, jump to merge
Push 6: then exit Sw spills     → Forward: at then exit
Push 7: then body PInsts        → Forward: then computation
Push 8: then entry Lw reloads   → Forward: at then entry
Push 9: head exit Sw spills     → Forward: before branches
Push A: head PInsts (BrIf→Beq)  → Forward: head (first)
```

Forward execution order after reversal:
```
[head computation + BrIf]
[Sw head exit spills]
[Lw then entry reloads] [then computation] [Sw then exit] [J merge]
[else_label] [Lw else entry reloads] [else computation] [Sw else exit]
[merge_label] [Lw merge reloads]
[rest...]
```

### Loop Backward Push Ordering

```
Push 0: post-loop Lw reloads     → Forward: after loop (last)
Push 1: PInst::Label(exit)       → Forward: exit label
Push 2: PInst::J(header)         → Forward: back-edge jump
Push 3: body exit Sw spills      → Forward: at body exit
Push 4: body PInsts               → Forward: body computation
Push 5: body entry Lw reloads    → Forward: at body entry
Push 6: PInst::Label(header)     → Forward: header label
Push 7: header Sw spills         → Forward: before body
Push 8: header PInsts             → Forward: header (first)
```

## Phases

1. Refactor lowering (Region labels, pure computation bodies)
2. RegPool boundary helpers + walk_ite
3. walk_loop
4. Sret calls + param precoloring
5. Filetest validation + cleanup

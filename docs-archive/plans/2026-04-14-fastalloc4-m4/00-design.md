# M4: Control Flow — Design

## Scope

Extend the fastalloc backward-walk allocator and emitter to handle
**IfThenElse**, **Loop**, and **Seq** regions. After M4, the allocator handles
any region tree produced by lowering. All filetests pass under `rv32fa.q32`.

## File Structure

```
lp-shader/lpvm-native/src/
├── fa_alloc/
│   ├── mod.rs              # UPDATE: allocate() dispatches walk_region
│   ├── walk.rs             # UPDATE: walk_region, walk_ite, walk_loop, walk_seq, boundary_spill
│   ├── liveness.rs         # MINOR: verify live_in correctness
│   ├── pool.rs             # MINOR: add pool.clear() or pool.drain_live()
│   ├── spill.rs            # No changes
│   ├── render.rs           # No changes
│   ├── verify.rs           # No changes
│   └── test/
│       ├── mod.rs          # UPDATE: control flow snapshot tests
│       └── builder.rs      # UPDATE: extend builder for ITE/Loop regions
├── lower.rs                # UPDATE: emit Label(continuing) as VInst
├── rv32/
│   └── emit.rs             # UPDATE: region-tree walk for emission
└── emit.rs                 # MINOR: pass region tree to rv32 emitter
```

## Architecture

### Allocator: recursive region walk (backward)

```
                    allocate(lowered, func_abi)
                              │
                              ▼
                     walk_region(root)
                      ┌───────┼───────┐
                      │       │       │
                  Linear   Seq    IfThenElse / Loop
                      │       │       │
              walk_linear  for child  boundary_spill()
              (existing)   in reverse  + walk_region(sub)
                              │       │
                         boundary_spill()
                         between children
```

`walk_region(tree, region_id, ...)` dispatches by region type:

- **Linear** → existing backward walk over `vinsts[start..end]`
- **Seq** → walk children in reverse order, `boundary_spill()` between each
- **IfThenElse** → spill at merge, walk else (empty pool), spill, walk then
  (empty pool), spill, walk head
- **Loop** → spill at exit, walk body (empty pool), spill at back-edge, walk
  header (empty pool), spill at entry

### Boundary spill protocol

At each region boundary:

```rust
fn boundary_spill(pool, spill, live_in, edits, anchor_idx):
    for each preg in pool where vreg ∈ live_in(dest_region):
        slot = spill.get_or_assign(vreg)
        edits.push(After(anchor_idx), Move(Reg(preg) → Stack(slot)))
        pool.free(preg)
```

- Spills only values in `live_in` of the destination region (liveness-guided)
- Values not live across the boundary are freed without a store
- Each sub-region starts with an empty pool, reloads on demand via `alloc_use`
- Matches regalloc2 fastalloc's invariant: "liveins arrive in spillslots"

### Emitter: region tree walk (forward)

```
    emit_region(tree, region_id)
        Linear  → emit vinsts[start..end] with alloc edits
        Seq     → emit children in order
        ITE     → emit head → emit then → emit Br(merge) →
                   emit Label(else) → emit else → emit Label(merge)
        Loop    → emit Br(header) → emit Label(header) →
                   emit header → emit body → emit Br(header) →
                   emit Label(exit)
```

Labels and structural branches come from `Region` node fields
(`else_label`, `merge_label`, `header_label`, `exit_label`). No new VInst
types needed. The emitter processes boundary edits between regions.

### Loop continue semantics

The lowerer emits `Label(continuing)` as a VInst at the appropriate position
in the body range. `Continue` → `Br(continuing)` resolves against it. The
allocator treats Label as a no-op. This is the same approach as the old backend.

### Alloc output format

Unchanged — flat `allocs` array with global VInst indices. The recursive walker
populates it region by region. `inst_alloc_offsets[i]` works as-is.

## Key Design Decisions

1. **Spill-at-boundary** (same cost as regalloc2 fastalloc): liveins arrive in
   spillslots, reload on demand.
2. **Liveness-guided spills**: only spill values in `live_in(dest)`, not
   everything in the pool. Dead values are freed without a store.
3. **Emitter owns control flow**: labels/branches at region boundaries are
   emitted by the emitter walking the region tree, not by the allocator.
4. **Lowerer emits `Label(continuing)`**: keeps Br/Label pairs self-consistent
   in the VInst stream.
5. **No parallel move resolver**: spill-to-slot avoids the need for one.

## Phases

| Phase | File | What |
|-------|------|------|
| 1 | `01-phase-lowering-fix.md` | Emit `Label(continuing)` in lowerer |
| 2 | `02-phase-walk-dispatch.md` | `walk_region` dispatch, `WalkState`, `boundary_spill`, Seq |
| 3 | `03-phase-ite.md` | IfThenElse allocation + emitter verification |
| 4 | `04-phase-loop.md` | Loop allocation (back-edges, loop-carried values) |
| 5 | `05-phase-emitter-fixup.md` | Label resolution, branch offset, edit anchoring verification |
| 6 | `06-phase-filetest-validation.md` | Full filetest suite triage + fixes |

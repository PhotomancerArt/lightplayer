# M2: Straight-Line Allocator - Notes

## Scope of Work

Implement backward walk allocator for Linear regions only (no calls, no control flow).
Produce per-operand allocations and edit list. Unit tested with snapshot tests.
Filetests pass for straight-line functions.

## Current State

After M1 (Gut + Prep):
- `fa_alloc/mod.rs`: Has Alloc, EditPoint, Edit, AllocOutput types defined
- `fa_alloc/pool.rs`: RegPool with LRU eviction extracted and working
- `rv32/emit.rs`: EmitContext skeleton ported (prologue/epilogue working)
- `debug/vinst.rs`: VInst text parser exists (`parse()` function)
- `allocate()` in mod.rs: Currently stubbed, returns NotImplemented

## Key Files
- `fa_alloc/walk.rs` - NEW: Backward walk allocator for Linear regions
- `fa_alloc/render.rs` - NEW: Human-readable AllocOutput rendering
- `fa_alloc/mod.rs` - UPDATE: Wire up walk, add snapshot tests

## Questions

### Q1: Snapshot Test Format [ANSWERED]

**Agreed format** - Separators with `;` comments:

```
; v0 in t0 (spill slot 0)
i0 = IConst32 10
; ---------------------------
; v1 in t1 (spill slot 1)
i1 = IConst32 20
; spill: v0 -> slot 0
; ---------------------------
; v2 in t0
i2 = Add32 i0, i1
; ---------------------------
Ret i2
```

- `;` for comments (assembly style)
- Separators between instruction blocks
- Register allocation shown at start of each block
- Spill/reload edits as comments between blocks

### Q2: Entry Parameter Handling [ANSWERED]

**Agreed: Option B** - Seed RegPool with params at ABI registers, record moves only if param moved.

Implementation:
1. Before walk: `pool.alloc_fixed(abi_reg, vreg)` for each param
2. After walk: For each param, compare `pool.home(vreg)` vs original ABI reg
3. If different: record `Edit::Move { from: Alloc::Reg(abi), to: Alloc::Reg(final) }` at `EditPoint::Before(0)`

This matches regalloc2's approach and avoids unnecessary moves.

### Q3: Edit Recording During Backward Walk [ANSWERED]

**Agreed: Option A** - Record edits during walk (in reverse order), reverse at the end.

This matches regalloc2's approach (they call `self.state.edits.reverse()` at the end of `run()`).

### Q4: Spill Slot Management [ANSWERED]

**Agreed: Option A** - Assign spill slots during the backward walk when first needed.

The existing `SpillAlloc` in `fa_alloc/spill.rs` tracks slot assignments. When evicting a vreg,
immediately get or create its spill slot.

### Q5: Operand Ordering Convention [ANSWERED]

**Agreed: Columnar format** - reads before instruction, writes after:

```
; ---------------------------
; read: i0 <- t1
; read: i1 <- t2
i2 = Add32 i0, i1
; write: i2 -> t0
```

- Separators clearly mark instruction boundaries
- Reads (uses) shown before the instruction  
- Writes (defs) shown after the instruction
- Spill/reload edits as comments between instruction blocks

# FastAlloc v4 Roadmap — Edit-List Architecture

## Motivation

The fastalloc3 allocator attempted direct PInst emission during the backward
walk over the region tree. This proved fundamentally flawed: later backward
walk decisions (evictions, register reassignment) invalidate register
assignments in already-emitted instructions. The value a register holds in
forward execution doesn't match what the backward walk assumed when it emitted
the instruction. This is not a fixable bug — it's an architectural limitation
of interleaving allocation decisions with instruction emission.

The fix is to follow regalloc2's proven approach: **separate allocation
decisions from instruction emission.** The backward walk produces per-operand
allocations and an edit list (moves, spills, reloads). A forward pass then
reads the VInst stream and the allocation plan to emit machine code bytes
directly. This also eliminates the PInst intermediary — the old `lpvm-native`
emitter proves that going directly from VInst + allocation → bytes works.

## Architecture

```
LPIR
 │  lower.rs (KEEP — ~1780 lines)
 ▼
VInst[] + RegionTree
 │  peephole.rs (KEEP)
 ▼
VInst[] (optimized)
 │  fa_alloc/              ← REWRITE
 │  ├── mod.rs             entry point, AllocOutput type
 │  ├── walk.rs            backward walk → per-operand allocs + edits
 │  ├── pool.rs            RegPool (LRU register pool)
 │  ├── spill.rs           SpillAlloc (KEEP)
 │  ├── liveness.rs        region-tree liveness (KEEP)
 │  └── trace.rs           AllocTrace (KEEP, may extend)
 ▼
AllocOutput { allocs, inst_alloc_offsets, edits, num_spill_slots, trace }
 │  rv32/emit.rs           ← NEW: ported from lpvm-native EmitContext
 │                            forward walk: VInst + AllocOutput → bytes
 ▼
machine code bytes (via rv32/encode.rs, KEEP)
```

### Key types (following regalloc2)

```rust
/// Where an operand lives: physical register, spill slot, or unassigned.
enum Alloc { Reg(PReg), Stack(u8), None }

/// Allocator output — per-operand assignments + edit list.
struct AllocOutput {
    /// Flat array of allocations, one per operand across all instructions.
    allocs: Vec<Alloc>,
    /// Index into `allocs` for each VInst's operands.
    inst_alloc_offsets: Vec<u16>,
    /// Moves to insert between instructions, sorted by program point.
    edits: Vec<(EditPoint, Edit)>,
    /// Total spill slots used.
    num_spill_slots: u32,
    /// Allocator decision trace for debugging.
    trace: AllocTrace,
}

/// Position relative to a VInst where an edit is inserted.
enum EditPoint { Before(u16), After(u16) }

/// An edit is a move between allocations (covers spill, reload, reg-reg).
enum Edit { Move { from: Alloc, to: Alloc } }
```

Per-operand allocation (not global vreg→preg) is required because a vreg can
be evicted from one register and reloaded into a different one. The linear scan
approach (one register per vreg, global) avoids this but produces worse code.

### What gets deleted

| File | Reason |
|------|--------|
| `fa_alloc/walk.rs` | Broken direct-emission architecture |
| `rv32/inst.rs` | PInst type no longer needed |
| `rv32/rv32_emit.rs` | PInst → bytes emitter, replaced by ported emitter |
| `rv32/debug/pinst.rs` | PInst debug formatting |

### What gets ported

The `EmitContext` from `lpvm-native/isa/rv32/emit.rs` (~1400 lines) handles
forward emission from VInst + allocation → bytes. It supports calls, sret,
branch fixups, spill loads/stores via the `use_vreg`/`def_vreg` pattern. This
is proven code. Adapt for FA crate's VInst types (VReg u16 vs u32, VRegSlice
vs Vec, SymbolId vs SymbolRef).

### Testing

Snapshot-style unit tests using the VInst text parser (`debug/vinst.rs`).
Render AllocOutput as human-readable text showing edits interleaved with
annotated VInsts. Compare against expected string. When a test fails, print
the actual output; copy-paste to bless.

```
expect_alloc("
    i0 = IConst32 10
    i1 = IConst32 20
    i2 = Add32 i0, i1
    Ret i2
", "
mv s5 -> slot[0]
i0 = IConst32 10          # i0→s3
---
i1 = IConst32 20          # i1→s5
---
i2 = Add32 i0, i1         # i0→s3 i1→s5 i2→s3
---
Ret i2                     # i2→s3
");
```

Filetests remain for integration validation (end-to-end correctness).

## Alternatives Considered

**Global vreg→preg table (linear scan style).** Simpler emitter but can't
represent eviction — a vreg evicted from s3 and reloaded into s9 requires
per-operand tracking. Produces worse code under register pressure (spilled
vregs stay spilled and use temp registers at every access).

**Direct PInst emission (fastalloc3).** Already proven broken. Later backward
decisions invalidate earlier emissions. The entire motivation for this roadmap.

**Rewrite from scratch (new crate).** Unnecessary — lowering, VInst,
RegionTree, encoding, ABI, runtimes, linking are all solid. Only the allocator
and emitter need replacement.

## Risks

- **Region-tree walk producing edits.** regalloc2 walks a CFG; we walk a
  region tree. The spill-at-boundary semantics for IfThenElse/Loop need
  careful translation to edit-list form. This is the main novelty.

- **Emitter port.** The old emitter uses `lpir::VReg` (u32) and `Vec<VReg>`
  for call args. Adapting to `VReg(u16)` and `VRegSlice` is mechanical but
  touches many match arms. Risk of subtle type conversion bugs.

- **Per-operand allocation vs simple table.** The emitter needs to look up
  allocations per-operand rather than per-vreg. The old emitter's
  `use_vreg`/`def_vreg` pattern needs adaptation to read from the alloc
  table instead of a global vreg→preg map.

## Milestones

| M   | Name            | Scope                                                    |
|-----|-----------------|----------------------------------------------------------|
| M1  | Gut + Prep      | Delete broken code, define types, port emitter, stubs    |
| M2  | Straight-line   | Backward walk for Linear, per-operand allocs, unit tests |
| M3  | Calls + Sret    | Call clobbers, arg/ret ABI, sret, stack-passed args      |
| M4  | Control Flow    | IfThenElse, Loop, spill-at-boundary edits                |
| M5  | Cleanup         | Remove lpvm-native, rename lpvm-native-fa                |

## Success Criteria

1. All filetests pass under `rv32fa` target (27/27)
2. Allocator unit tests cover straight-line, spill, call, sret, if/else, loop
3. Snapshot tests are readable and blessable
4. AllocTrace output is useful for debugging allocation decisions
5. `lpvm-native` (cranelift) crate fully removed
6. No regressions in firmware build (`fw-esp32`, `fw-emu`)

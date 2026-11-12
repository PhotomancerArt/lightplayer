# FastAlloc v4 M1 — Gut + Prep Design

## Scope

Delete the broken direct-emission allocator and PInst layer. Define the new
AllocOutput types following regalloc2. Port the old `lpvm-native` forward
emitter as `rv32/emit.rs`. Leave stubs for the actual allocator.

## File Structure

```
lp-shader/lpvm-native/
└── src/
    ├── fa_alloc/
    │   ├── mod.rs              # UPDATE: AllocOutput types, stub allocator
    │   ├── spill.rs            # KEEP: SpillAlloc
    │   ├── liveness.rs         # KEEP: region-tree liveness
    │   ├── trace.rs            # KEEP: AllocTrace
    │   ├── pool.rs             # NEW: RegPool (extracted from old walk.rs)
    │   ├── walk.rs             # DELETE: (old broken walk - 1633 lines)
    │   └── render.rs           # (M2) AllocOutput rendering for tests
    │
    ├── rv32/
    │   ├── mod.rs              # UPDATE: remove PInst re-exports
    │   ├── encode.rs           # KEEP: instruction encoders
    │   ├── gpr.rs              # KEEP: register constants
    │   ├── abi.rs              # KEEP: ABI definitions
    │   ├── inst.rs             # DELETE: (old PInst - 240 lines)
    │   ├── rv32_emit.rs        # DELETE: (old PInst emitter)
    │   ├── emit.rs             # NEW: ported from lpvm-native
    │   └── debug/
    │       ├── mod.rs          # KEEP
    │       ├── disasm.rs       # KEEP
    │       ├── pinst.rs        # DELETE: (PInst debug)
    │       └── region.rs       # KEEP
    │
    ├── rv32.rs                 # UPDATE: remove inst module
    │
    └── emit.rs                 # UPDATE: call new allocator + new emitter
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    VInst[] + RegionTree                 │
│                   (from lower.rs, peephole)             │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   fa_alloc::mod.rs                      │
│  ┌─────────────────────────────────────────────────┐    │
│  │  Types: Alloc, AllocOutput, Edit, EditPoint     │    │
│  │  Stub: allocate() -> Err(NotImplemented)          │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   rv32::emit.rs (NEW)                   │
│  ┌─────────────────────────────────────────────────┐    │
│  │  EmitContext: forward walk VInst + AllocOutput   │    │
│  │  use_vreg():  load from spill if needed          │    │
│  │  def_vreg():  allocate temp if spilled           │    │
│  │  Calls: auipc+jalr, arg/ret moves, sret handling │    │
│  │  Branches: fixup recording, offset patching      │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    machine code bytes                   │
│                   (via rv32::encode)                    │
└─────────────────────────────────────────────────────────┘
```

## Main Components

### 1. AllocOutput Types (`fa_alloc/mod.rs`)

Following regalloc2's `Output` structure but simplified for RV32 integer-only:

- **`Alloc`**: Where an operand lives — physical register, spill slot, or none
- **`AllocOutput`**: Per-operand allocations + sorted edit list + spill slot count
- **`EditPoint`**: Position relative to a VInst (Before/After)
- **`Edit`**: Move between allocations (covers spill, reload, reg-reg move)

The allocator (M2) produces this. The emitter (M1) reads it.

### 2. Forward Emitter (`rv32/emit.rs`)

Ported from `lpvm-native/src/isa/rv32/emit.rs`. Key patterns:

- **`use_vreg(alloc, vreg, temp)`**: Get the physical register for a use. If the
  vreg is spilled (Allocation is Stack), load from spill slot into `temp` and
  return `temp`. If rematerializable (IConst32), emit immediate sequence into
  `temp`.

- **`def_vreg(alloc, vreg, temp)`**: Get the physical register for a def. If
  spilled, return `temp` as the temporary register. Caller must store after.

- **`store_def_vreg(alloc, vreg, temp)`**: If vreg was spilled, store `temp`
  to the spill slot.

- **Call emission**: Handle arg moves to ABI registers, auipc+jalr pair,
  return value moves, clobber save/restore (via edits from allocator).

- **Branch fixups**: Record label positions, patch branch offsets after all
  labels are known.

### 3. Deletions

- `fa_alloc/walk.rs`: Entire 1633-line file. Broken architecture.
- `rv32/inst.rs`: PInst type definition. No longer needed.
- `rv32/rv32_emit.rs`: PInst → bytes encoder. Replaced by direct VInst → bytes.
- `rv32/debug/pinst.rs`: PInst debug formatting.

### 4. Adaptations from Old Emitter

- VReg: `lpir::VReg` (u32) → `VReg(u16)` — just type aliases, change field access
- Call args/rets: `Vec<VReg>` → `VRegSlice` — iterate via `slice.vregs(pool)`
- Call target: `SymbolRef` → `SymbolId` + `ModuleSymbols` — look up name
- Allocation lookup: `alloc.vreg_to_phys[vreg]` → `allocs[(inst_idx, operand_idx)]`

## Interactions

1. **`emit.rs` (orchestration)** calls `fa_alloc::allocate()` — currently stub,
   will return error during M1.

2. **Future (M2)**: `allocate()` walks region tree backward, fills in
   `AllocOutput`, returns success. `emit.rs` passes `AllocOutput` to new
   `rv32::emit.rs` forward emitter.

3. **Forward emitter** walks VInsts, looks up allocations per-operand, emits
   bytes via `rv32::encode` encoders.

## Success Criteria

- `cargo check -p lpvm-native` passes
- All old broken code deleted
- New types defined
- Emitter ported and compiling
- Stub allocator returns clear error message

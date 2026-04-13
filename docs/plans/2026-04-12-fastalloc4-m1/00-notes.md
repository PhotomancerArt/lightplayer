# FastAlloc v4 M1 — Gut + Prep Notes

## Scope

Delete the broken direct-emission allocator and PInst layer. Define the new
AllocOutput types. Port the old `lpvm-native` forward emitter as `rv32/emit.rs`.
Leave stubs for the actual allocator. The crate compiles but allocation returns
an error.

## Current State

### Files to delete

- `lp-shader/lpvm-native-fa/src/fa_alloc/walk.rs` (1633 lines)
  - Contains broken backward walk with direct PInst emission
  - Has `RegPool` utility that we'll extract first
  - Has `emit_vinst` VInst→PInst mapping that we won't need (going direct to bytes)

- `lp-shader/lpvm-native-fa/src/rv32/inst.rs` (240 lines)
  - PInst enum definition

- `lp-shader/lpvm-native-fa/src/rv32/rv32_emit.rs`
  - PInst → bytes encoder

- `lp-shader/lpvm-native-fa/src/rv32/debug/pinst.rs`
  - PInst debug formatting

### Files to keep (reference for porting)

- `lp-shader/lpvm-native/src/isa/rv32/emit.rs` (~1400 lines)
  - `EmitContext` — forward emitter from VInst + allocation → bytes
  - `use_vreg`, `def_vreg`, `store_def_vreg` pattern for spill handling
  - Call emission (auipc+jalr), sret handling, branch fixups
  - Adapt for FA VInst types (VReg u16, VRegSlice, SymbolId)

### Files to modify

- `lp-shader/lpvm-native-fa/src/fa_alloc/mod.rs`
  - Replace `run_shell()` and `allocate()` with stub that returns `Err`
  - Define `AllocOutput`, `Alloc`, `Edit`, `EditPoint` types

- `lp-shader/lpvm-native-fa/src/emit.rs`
  - Call new allocator (stub) and new emitter

- `lp-shader/lpvm-native-fa/src/rv32/mod.rs`
  - Remove PInst re-exports

### Types to define

```rust
// fa_alloc/mod.rs

pub enum Alloc {
    Reg(PReg),
    Stack(u8),
    None,
}

pub struct AllocOutput {
    /// Flat array: allocs[(inst_idx, operand_idx)]
    pub allocs: Vec<Alloc>,
    /// Offset into allocs for each instruction's operands
    pub inst_alloc_offsets: Vec<u16>,
    /// Sorted edits: (point, edit)
    pub edits: Vec<(EditPoint, Edit)>,
    /// Total spill slots needed
    pub num_spill_slots: u32,
    /// Trace for debugging
    pub trace: AllocTrace,
}

pub enum EditPoint {
    Before(u16),  // VInst index
    After(u16),
}

pub enum Edit {
    Move { from: Alloc, to: Alloc },
}
```

## Questions

### Q1: How much of the old `lpvm-native` emitter to port vs adapt?

**Context:** The old emitter is ~1400 lines. It includes:
- `EmitContext` struct and methods (~200 lines)
- Prologue/epilogue emission (~100 lines)
- Per-VInst emission handlers (~900 lines, one match arm per VInst variant)
- Call emission (direct and sret) (~150 lines)
- Branch fixup handling (~50 lines)

The FA crate's VInst differs in:
- `VReg` is `u16` not `lpir::VReg` (u32)
- Call args/rets use `VRegSlice` not `Vec<VReg>`
- Call target is `SymbolId` not `SymbolRef`
- Some VInst variants may differ slightly

**Answer:** Port the entire emitter structurally but adapt types inline.
Don't try to abstract over the differences — the VInst types are close enough
that mechanical adaptation is simpler than a compatibility layer. Keep the
`use_vreg`/`def_vreg` pattern but change them to read from `AllocOutput` instead
of the old `Allocation` struct.

### Q2: Do we keep `rv32/encode.rs`?

**Context:** `rv32/encode.rs` contains the instruction encoders (`encode_add`,
`encode_lw`, etc.) that both the old and new emitters use.

**Answer:** Yes, keep it. The encoders are pure functions that don't
depend on PInst. They'll be used by the new `rv32/emit.rs`.

### Q3: How to handle the transition where `compile.rs` calls the allocator?

**Context:** `compile.rs` calls `fa_alloc::allocate()` which currently tries
to do real allocation and will be broken during M1.

**Answer:** Make `allocate()` return `Err(AllocError::NotImplemented)`
with a clear message like "M1: allocator not yet implemented". `compile.rs`
propagates the error. Not having working compilation during M1 is expected.

### Q4: What about `debug/vinst.rs` text parser — any changes needed?

**Context:** The VInst text parser is used for testing. It produces `VInst[]`.

**Suggested answer:** No changes needed. The parser is already working and
produces the correct VInst types. It will be used in M2 for snapshot tests.

## Notes

- The old `lpvm-native` crate is at `lp-shader/lpvm-native/`. We'll port from
  `src/isa/rv32/emit.rs` there.
- Keep the `rv32/encode.rs` encoders unchanged.
- The `emit.rs` orchestration file (the one that calls the allocator) is thin
  and just needs to call the new types.
- Goal for M1: `cargo check -p lpvm-native-fa` passes. Tests may fail (expected).

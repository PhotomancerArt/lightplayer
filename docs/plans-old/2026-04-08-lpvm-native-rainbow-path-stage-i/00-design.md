# M1: ABI Design - sret, Multi-Return, Out-Params

## Scope of Work

Implement the RISC-V RV32 ILP32 calling convention for LPIR:

- **Multi-scalar returns**: Return small structs/vectors in registers (vec2 in a0-a1, vec4 in a0-a3)
- **`sret` pointer**: For large returns (>16 bytes), pass destination address in a0
- **Stack slots**: LPIR stack slot allocation, address computation for sret buffers
- **Out-parameters**: Pointer arguments use standard register passing (no ABI changes)
- **Stack spill slots**: Frame layout with s0-relative negative offsets
- **Frame management**: Prologue/epilogue with saved ra, frame pointer, spill area
- **Greedy+spill**: Emergency spill support in greedy allocator

## File Structure

```
lp-shader/lpvm-native/src/
├── lib.rs                      # UPDATE: Re-export new ABI types
├── error.rs                    # UPDATE: Add spill-related errors
├── types.rs                    # UPDATE: Add spill slot tracking
├── regalloc/
│   ├── mod.rs                  # UPDATE: Add spill slot to Allocation
│   └── greedy.rs               # UPDATE: Add emergency spill support
├── isa/
│   └── rv32/
│       ├── mod.rs
│       ├── inst.rs             # (no change)
│       ├── abi.rs              # UPDATE: Return classification, spill frame layout
│       └── emit.rs             # UPDATE: sret, multi-return, spill code emission
└── vinst.rs                    # UPDATE: Add LoadSpill, StoreSpill variants
```

## Conceptual Architecture

### ABI Layer (`isa/rv32/abi.rs`)

```
┌─────────────────────────────────────────────────────────┐
│  Return Classification                                  │
│  - FnSig → ReturnClass (Direct {regs}, Sret {ptr_reg}) │
│  - >16 bytes: sret pointer in a0                        │
│  - ≤16 bytes: a0-a3 direct                              │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│  Frame Layout (QBE-style)                               │
│  - s0 = frame pointer (points to saved s0)            │
│  - ra at s0+4                                           │
│  - spills at negative offsets: s0-8, s0-12, ...       │
│  - 4-byte slots, 16-byte aligned total                  │
└─────────────────────────────────────────────────────────┘
```

### Register Allocation (`regalloc/greedy.rs`)

```
┌─────────────────────────────────────────────────────────┐
│  Greedy Allocator + Emergency Spill                     │
│  - Round-robin allocation (x8-x31)                      │
│  - When exhausted: assign spill slot (index 0, 1, 2...) │
│  - Track spill_slot_count in Allocation                 │
└─────────────────────────────────────────────────────────┘
```

### Emission (`isa/rv32/emit.rs`)

```
┌─────────────────────────────────────────────────────────┐
│  Prologue (non-leaf)                                    │
│  - addi sp, sp, -frame_size                             │
│  - sw   ra, 4(sp)      # ra at s0+4                    │
│  - sw   s0, 0(sp)      # s0 points here                │
│  - addi s0, sp, 0                                       │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  Multi-Value Return                                       │
│  - mv a0, vreg_0_result                                 │
│  - mv a1, vreg_1_result   (if 2+ scalars)              │
│  - mv a2, vreg_2_result   (if 3+ scalars)              │
│  - mv a3, vreg_3_result   (if 4 scalars)               │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  Spill Code                                               │
│  - sw   vreg, offset(s0)    # store to spill slot       │
│  - lw   vreg, offset(s0)    # load from spill slot      │
│  - offset = -8, -12, -16, ... (4-byte aligned)          │
└─────────────────────────────────────────────────────────┘
```

### VInst Layer (`vinst.rs`)

Add spill-specific VInsts for emergency spill/reload:

```rust
enum VInst {
    // ... existing ...
    LoadSpill { dst: VReg, slot: SpillSlot, src_op: Option<u32> },
    StoreSpill { src: VReg, slot: SpillSlot, src_op: Option<u32> },
}
```

## Main Components and How They Interact

1. **Return Classification** (`abi.rs`): Analyzes LPIR return types, classifies as Direct (a0-a3) or Sret (a0 pointer). Used by emit to generate proper return sequence.

2. **Stack Slot Layout** (`abi.rs`): Computes stack slot offsets for LPIR `StackSlot` data. Tracks slot size, alignment, assigns offsets from s0.

3. **Frame Layout** (`abi.rs`): Computes total frame size from spill count + stack slots, provides `spill_to_offset(slot)` function for emit. QBE-style negative offsets from s0.

4. **Greedy Allocator** (`greedy.rs`): Allocates registers round-robin. When register pool exhausted, assigns spill slot index (0, 1, 2...). Returns `Allocation` with `spill_slot_count`.

5. **Spill Code Emission** (`emit.rs`): For LoadSpill/StoreSpill VInsts, computes s0-relative offset (`-8 - slot*4`) and emits `lw`/`sw`.

6. **Multi-Return Emission** (`emit.rs`): Iterates return values, moves each to a0-a3 based on position. For >4 scalars, expects sret setup (caller-allocated buffer).

7. **VInst Lowering** (future): Will lower LPIR returns to multi-return moves, lower `StackSlotAddr` to s0-relative addressing, insert LoadSpill/StoreSpill around uses/defs of spilled vregs.

## Key Decisions

- **Frame pointer**: s0-relative with negative offsets (QBE-style)
- **Spill slot size**: 4 bytes (I32/F32), aligned to 4
- **Return threshold**: 16 bytes (4 scalars on RV32)
- **Greedy spill**: Emergency-only, round-robin slot assignment
- **Out-params**: Normal pointer args, no ABI changes

## Filetest Strategy

**Existing tests to verify**: 
- `vec/` tests for vec2/vec3/vec4 returns
- `mat/` tests for mat4 (16 scalars = sret threshold edge case)

**New spill test** (`filetests/scalar/spill_pressure.glsl`):
```glsl
// Force spilling by using many simultaneous values
mat4 test_spill_many_mat4() {
    mat4 a = mat4(1.0);
    mat4 b = mat4(2.0);
    mat4 c = mat4(3.0);
    mat4 d = mat4(4.0);  // Each mat4 = 16 scalars, 4 mat4s = 64 vregs
    return a + b + c + d;  // Forces many live values, must spill
}
```

This test will generate heavy spill traffic with greedy allocator, giving us baseline instruction counts for comparison with linear scan later.

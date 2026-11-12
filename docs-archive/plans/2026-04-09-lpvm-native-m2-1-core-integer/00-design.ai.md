# Design: lpvm-native M2.1 Core Integer Operations

## Scope of Work

Implement lowering and emission for core integer operations:

1. **Division and remainder**: `IdivS`, `IdivU`, `IremS`, `IremU` → RV32 `div`/`divu`/`rem`/`remu`
2. **Integer comparisons**: `Ieq`, `Ine`, `IltS`, `IleS`, `IgtS`, `IgeS`, `IltU`, `IleU`, `IgtU`, `IgeU` → RV32 slt sequences
3. **Selection**: `Select` → branchless arithmetic expansion

## File Structure

```
lp-shader/lpvm-native/src/
├── vinst.rs                     # UPDATE: Add DivS32, DivU32, RemS32, RemU32,
│                                #         Icmp32, IcmpCond, Select32
├── lower.rs                     # UPDATE: Handle IdivS/U, IremS/U, Icmp*, Select
├── isa/rv32/inst.rs             # UPDATE: encode_div, encode_divu, encode_rem, encode_remu,
│                                #         encode_slt, encode_sltu, encode_snez
└── isa/rv32/emit.rs             # UPDATE: Emit new VInsts, expand Select to arithmetic
```

## Conceptual Architecture

### Lowering Flow

```
LPIR Op                    →    VInst(s)                    →    RV32 Instructions
─────────────────────────────────────────────────────────────────────────────────────
IdivS {dst, lhs, rhs}      →    DivS32 {dst, lhs, rhs}      →    div rd, rs1, rs2
IdivU {dst, lhs, rhs}      →    DivU32 {dst, lhs, rhs}      →    divu rd, rs1, rs2
IremS {dst, lhs, rhs}      →    RemS32 {dst, lhs, rhs}      →    rem rd, rs1, rs2
IremU {dst, lhs, rhs}      →    RemU32 {dst, lhs, rhs}      →    remu rd, rs1, rs2

Ieq/IltS/etc {dst,lhs,rhs} →    Icmp32 {dst,lhs,rhs,cond}   →    slt/sltu/xori seq
                                                   ↓
                                              (emission builds
                                               comparison result)

Select {dst,cond,t,f}      →    [expanded to multiple VInsts]
                              ├─ Sub32 {tmp1, if_true, if_false}
                              ├─ And32 {tmp2, tmp1, cond}
                              └─ Add32 {dst, tmp2, if_false}
```

### Key Design Decisions

1. **Separate div/rem VInsts for signed/unsigned**: Mirrors LPIR and RV32 structure
2. **Single Icmp32 with condition code**: Reduces VInst variants, clean emission logic
3. **Select expanded during lowering**: Branchless arithmetic sequence, no control flow dependency
4. **Division by zero**: Uses RV32 hardware-defined behavior (acceptable per GLSL UB rules)

### New VInsts

```rust
// Division and remainder
DivS32 { dst: VReg, lhs: VReg, rhs: VReg, src_op: Option<u32> }
DivU32 { dst: VReg, lhs: VReg, rhs: VReg, src_op: Option<u32> }
RemS32 { dst: VReg, lhs: VReg, rhs: VReg, src_op: Option<u32> }
RemU32 { dst: VReg, lhs: VReg, rhs: VReg, src_op: Option<u32> }

// Comparison
Icmp32 { dst: VReg, lhs: VReg, rhs: VReg, cond: IcmpCond, src_op: Option<u32> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpCond {
    Eq,     // Equal
    Ne,     // Not equal
    LtS,    // Signed less than
    LeS,    // Signed less than or equal
    GtS,    // Signed greater than
    GeS,    // Signed greater than or equal
    LtU,    // Unsigned less than
    LeU,    // Unsigned less than or equal
    GtU,    // Unsigned greater than
    GeU,    // Unsigned greater than or equal
}
```

### Emission Logic

**Division/remainder**: Direct 1:1 mapping to RV32 instructions.

**Comparisons**:
- `LtS`: `slt dst, lhs, rhs`
- `LtU`: `sltu dst, lhs, rhs`
- `Eq`: `xor tmp, lhs, rhs; sltiu dst, tmp, 1` (set if zero)
- `Ne`: `xor tmp, lhs, rhs; sltu dst, zero, tmp` (set if non-zero)
- `LeS`: `slt tmp, rhs, lhs; xori dst, tmp, 1` (not greater)
- etc.

**Select** (expanded during lowering, not emitted directly):
```rust
// Lowering transforms:
// Select { dst, cond, if_true, if_false }
// Into:
//   tmp1 = Sub32(if_true, if_false)
//   tmp2 = And32(tmp1, cond)
//   dst = Add32(tmp2, if_false)
```

## Main Components and How They Interact

1. **lower.rs**: Pattern matches on LPIR ops, produces VInsts. For Select, generates multiple VInsts.
2. **vinst.rs**: Defines new VInst variants and IcmpCond enum.
3. **isa/rv32/inst.rs**: RV32 instruction encoders for div/rem/slt.
4. **isa/rv32/emit.rs**: Emits VInsts to bytes. Handles each IcmpCond variant with appropriate instruction sequence.

## Acceptance Criteria

- `IdivS`, `IdivU`, `IremS`, `IremU` lower and emit correctly
- All 10 comparison ops produce correct boolean results
- `Select` produces correct conditional values
- Unit tests for each operation
- Filetests pass (icmp.glsl, select.glsl if available)

# Phase 3: Compact VInst Enum

## Scope

Rewrite the VInst enum to use the compact types:
- `VReg(pub u16)` instead of `lpir::VReg`
- `VRegSlice` instead of `Vec<VReg>` for Call/Ret
- `SymbolId` instead of `SymbolRef { String }`
- `src_op: u16` with `SRC_OP_NONE` sentinel instead of `Option<u32>`
- `LabelId = u16` instead of `u32`

## Implementation

### 1. Update VInst enum variants

Replace the entire enum (keeping same variant names for compatibility):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    // Arithmetic (3 registers + src_op = 8 bytes)
    Add32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Sub32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Neg32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    And32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Or32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Xor32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Bnot32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Shl32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    ShrS32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    ShrU32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },

    // Division/remainder (3 registers + src_op = 8 bytes)
    DivS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    DivU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    RemS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    RemU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },

    // Comparison (3 registers + 1 byte cond + padding + src_op = 10 bytes, aligns to 12)
    Icmp32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
        src_op: u16,
    },
    IeqImm32 {
        dst: VReg,
        src: VReg,
        imm: i32,
        src_op: u16,
    },
    Select32 {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
        src_op: u16,
    },

    // Branches (LabelId u16 + src_op u16 = 4 bytes)
    Br {
        target: LabelId,
        src_op: u16,
    },
    BrIf {
        cond: VReg,
        target: LabelId,
        invert: bool,
        src_op: u16,
    },

    // Data movement (2-3 registers + src_op = 6-8 bytes)
    Mov32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Load32 {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    Store32 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    SlotAddr {
        dst: VReg,
        slot: u32,
        src_op: u16,
    },
    MemcpyWords {
        dst_base: VReg,
        src_base: VReg,
        size: u32,
        src_op: u16,
    },

    // Constants (1 register + i32 + src_op = 8 bytes)
    IConst32 {
        dst: VReg,
        val: i32,
        src_op: u16,
    },

    // Call - the big win! No heap allocations
    // SymbolId (2) + VRegSlice × 2 (6) + sret (1) + padding + src_op (2) = ~14 bytes
    Call {
        target: SymbolId,
        args: VRegSlice,
        rets: VRegSlice,
        callee_uses_sret: bool,
        src_op: u16,
    },

    // Ret - also no heap allocation
    // VRegSlice (4) + src_op (2) + padding = 8 bytes
    Ret {
        vals: VRegSlice,
        src_op: u16,
    },

    // Label (LabelId u16 + Option<u32> was 8 bytes, now 4)
    Label(LabelId, u16),  // (label, src_op)
}
```

### 2. Update src_op() method

```rust
impl VInst {
    /// Index of the originating LPIR op, or SRC_OP_NONE if not tracked.
    pub fn src_op(&self) -> u16 {
        match self {
            VInst::Add32 { src_op, .. }
            | VInst::Sub32 { src_op, .. }
            | VInst::Neg32 { src_op, .. }
            | VInst::Mul32 { src_op, .. }
            | VInst::And32 { src_op, .. }
            | VInst::Or32 { src_op, .. }
            | VInst::Xor32 { src_op, .. }
            | VInst::Bnot32 { src_op, .. }
            | VInst::Shl32 { src_op, .. }
            | VInst::ShrS32 { src_op, .. }
            | VInst::ShrU32 { src_op, .. }
            | VInst::DivS32 { src_op, .. }
            | VInst::DivU32 { src_op, .. }
            | VInst::RemS32 { src_op, .. }
            | VInst::RemU32 { src_op, .. }
            | VInst::Icmp32 { src_op, .. }
            | VInst::IeqImm32 { src_op, .. }
            | VInst::Select32 { src_op, .. }
            | VInst::Br { src_op, .. }
            | VInst::BrIf { src_op, .. }
            | VInst::Mov32 { src_op, .. }
            | VInst::Load32 { src_op, .. }
            | VInst::Store32 { src_op, .. }
            | VInst::SlotAddr { src_op, .. }
            | VInst::MemcpyWords { src_op, .. }
            | VInst::IConst32 { src_op, .. }
            | VInst::Call { src_op, .. }
            | VInst::Ret { src_op, .. } => *src_op,
            VInst::Label(_, src_op) => *src_op,
        }
    }
}
```

### 3. Add size test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn test_vinst_size() {
        // Should be significantly smaller than old ~88 bytes
        let sz = size_of::<VInst>();
        assert!(sz <= 32, "VInst size {} exceeds 32 bytes", sz);
        // Ideal is ~20 bytes, but allow up to 32 for alignment
    }

    #[test]
    fn test_vreg_size() {
        assert_eq!(size_of::<VReg>(), 2);
    }

    #[test]
    fn test_vregslice_size() {
        assert_eq!(size_of::<VRegSlice>(), 4);
    }

    #[test]
    fn test_symbolid_size() {
        assert_eq!(size_of::<SymbolId>(), 2);
    }
}
```

## Code Organization Reminders

- Keep enum variants in same order as old file for easier diff review
- Place helper methods (src_op, mnemonic, etc.) after enum definition
- Place tests at bottom of file

## Validate

```bash
cargo test -p lpvm-native --lib -- vinst::tests::test_vinst_size
```

Should pass with VInst size ≤ 32 bytes (ideally ~20-24).

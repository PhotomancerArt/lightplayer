## Scope of Phase

Update `emit.rs` to emit new VInsts: And32, DivS32, DivU32, RemS32, RemU32, Icmp32, Select32.

## Code Organization Reminders

- Add match arms in `emit_vinst()` for new VInst variants
- For comparisons, emit appropriate instruction sequence for each condition
- For Select32, expand to arithmetic sequence inline
- Place encoder imports at top, emission logic in match order

## Implementation Details

### Changes to `isa/rv32/emit.rs`

Add imports for new encoders:

```rust
use super::inst::{
    encode_addi, encode_and, encode_auipc, encode_div, encode_divu,
    encode_jalr, encode_lw, encode_mul, encode_rem, encode_remu,
    encode_ret, encode_slt, encode_slti, encode_sltiu, encode_sub,
    encode_sw, encode_xor, encode_xori, iconst32_sequence,
};
```

Add match arms to `emit_vinst()`:

```rust
pub fn emit_vinst(
    &mut self,
    inst: &VInst,
    alloc: &Allocation,
    is_sret: bool,
) -> Result<(), NativeError> {
    self.current_src_op = inst.src_op();
    match inst {
        // ... existing arms ...

        // Bitwise AND
        VInst::And32 { dst, src1, src2, .. } => {
            let rs1 = self.use_vreg(alloc, *src1, Self::TEMP0)? as u32;
            let rs2 = self.use_vreg(alloc, *src2, Self::TEMP1)? as u32;
            let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
            self.push_u32(encode_and(rd, rs1, rs2));
            self.store_def_vreg(alloc, *dst, Self::TEMP0);
        }

        // Division and remainder
        VInst::DivS32 { dst, lhs, rhs, .. } => {
            let rs1 = self.use_vreg(alloc, *lhs, Self::TEMP0)? as u32;
            let rs2 = self.use_vreg(alloc, *rhs, Self::TEMP1)? as u32;
            let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
            self.push_u32(encode_div(rd, rs1, rs2));
            self.store_def_vreg(alloc, *dst, Self::TEMP0);
        }
        VInst::DivU32 { dst, lhs, rhs, .. } => {
            let rs1 = self.use_vreg(alloc, *lhs, Self::TEMP0)? as u32;
            let rs2 = self.use_vreg(alloc, *rhs, Self::TEMP1)? as u32;
            let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
            self.push_u32(encode_divu(rd, rs1, rs2));
            self.store_def_vreg(alloc, *dst, Self::TEMP0);
        }
        VInst::RemS32 { dst, lhs, rhs, .. } => {
            let rs1 = self.use_vreg(alloc, *lhs, Self::TEMP0)? as u32;
            let rs2 = self.use_vreg(alloc, *rhs, Self::TEMP1)? as u32;
            let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
            self.push_u32(encode_rem(rd, rs1, rs2));
            self.store_def_vreg(alloc, *dst, Self::TEMP0);
        }
        VInst::RemU32 { dst, lhs, rhs, .. } => {
            let rs1 = self.use_vreg(alloc, *lhs, Self::TEMP0)? as u32;
            let rs2 = self.use_vreg(alloc, *rhs, Self::TEMP1)? as u32;
            let rd = self.def_vreg(alloc, *dst, Self::TEMP0)? as u32;
            self.push_u32(encode_remu(rd, rs1, rs2));
            self.store_def_vreg(alloc, *dst, Self::TEMP0);
        }

        // Comparisons
        VInst::Icmp32 { dst, lhs, rhs, cond, .. } => {
            self.emit_icmp(alloc, *dst, *lhs, *rhs, *cond)?;
        }

        // Select - expand to arithmetic sequence
        VInst::Select32 { dst, cond, if_true, if_false, .. } => {
            self.emit_select(alloc, *dst, *cond, *if_true, *if_false)?;
        }
    }
    self.current_src_op = None;
    Ok(())
}
```

Add helper methods at bottom of `EmitContext`:

```rust
/// Emit integer comparison with appropriate instruction sequence
fn emit_icmp(
    &mut self,
    alloc: &Allocation,
    dst: VReg,
    lhs: VReg,
    rhs: VReg,
    cond: IcmpCond,
) -> Result<(), NativeError> {
    let rs1 = self.use_vreg(alloc, lhs, Self::TEMP0)? as u32;
    let rs2 = self.use_vreg(alloc, rhs, Self::TEMP1)? as u32;
    let rd = self.def_vreg(alloc, dst, Self::TEMP0)? as u32;

    match cond {
        IcmpCond::LtS => {
            // slt rd, lhs, rhs
            self.push_u32(encode_slt(rd, rs1, rs2));
        }
        IcmpCond::LtU => {
            // sltu rd, lhs, rhs
            self.push_u32(encode_sltu(rd, rs1, rs2));
        }
        IcmpCond::Eq => {
            // xor tmp, lhs, rhs
            // sltiu rd, tmp, 1  (set if zero)
            self.push_u32(encode_xor(Self::TEMP1 as u32, rs1, rs2));
            self.push_u32(encode_sltiu(rd, Self::TEMP1 as u32, 1));
        }
        IcmpCond::Ne => {
            // xor tmp, lhs, rhs
            // sltu rd, zero, tmp  (set if non-zero)
            self.push_u32(encode_xor(Self::TEMP1 as u32, rs1, rs2));
            self.push_u32(encode_sltu(rd, 0, Self::TEMP1 as u32));
        }
        IcmpCond::LeS => {
            // slt tmp, rhs, lhs
            // xori rd, tmp, 1  (not greater)
            self.push_u32(encode_slt(Self::TEMP1 as u32, rs2, rs1));
            self.push_u32(encode_xori(rd, Self::TEMP1 as u32, 1));
        }
        IcmpCond::LeU => {
            // sltu tmp, rhs, lhs
            // xori rd, tmp, 1
            self.push_u32(encode_sltu(Self::TEMP1 as u32, rs2, rs1));
            self.push_u32(encode_xori(rd, Self::TEMP1 as u32, 1));
        }
        IcmpCond::GtS => {
            // slt rd, rhs, lhs  (reverse operands)
            self.push_u32(encode_slt(rd, rs2, rs1));
        }
        IcmpCond::GtU => {
            // sltu rd, rhs, lhs
            self.push_u32(encode_sltu(rd, rs2, rs1));
        }
        IcmpCond::GeS => {
            // slt tmp, lhs, rhs
            // xori rd, tmp, 1  (not less)
            self.push_u32(encode_slt(Self::TEMP1 as u32, rs1, rs2));
            self.push_u32(encode_xori(rd, Self::TEMP1 as u32, 1));
        }
        IcmpCond::GeU => {
            // sltu tmp, lhs, rhs
            // xori rd, tmp, 1
            self.push_u32(encode_sltu(Self::TEMP1 as u32, rs1, rs2));
            self.push_u32(encode_xori(rd, Self::TEMP1 as u32, 1));
        }
    }

    self.store_def_vreg(alloc, dst, Self::TEMP0);
    Ok(())
}

/// Emit Select as branchless arithmetic sequence:
/// result = if_false + ((if_true - if_false) & cond)
fn emit_select(
    &mut self,
    alloc: &Allocation,
    dst: VReg,
    cond: VReg,
    if_true: VReg,
    if_false: VReg,
) -> Result<(), NativeError> {
    // Load all sources
    let r_cond = self.use_vreg(alloc, cond, Self::TEMP0)? as u32;
    let r_true = self.use_vreg(alloc, if_true, Self::TEMP1)? as u32;
    let r_false = self.use_vreg(alloc, if_false, Self::TEMP2)? as u32;
    let rd = self.def_vreg(alloc, dst, Self::TEMP0)? as u32;

    // tmp1 = if_true - if_false
    self.push_u32(encode_sub(Self::TEMP1 as u32, r_true, r_false));

    // tmp2 = tmp1 & cond
    self.push_u32(encode_and(Self::TEMP2 as u32, Self::TEMP1 as u32, r_cond));

    // result = tmp2 + if_false
    self.push_u32(encode_add(rd, Self::TEMP2 as u32, r_false));

    self.store_def_vreg(alloc, dst, Self::TEMP0);
    Ok(())
}
```

**Note**: Need to add `TEMP2` constant:

```rust
/// Temporary registers for spill handling.
const TEMP0: PhysReg = 5; // t0
const TEMP1: PhysReg = 6; // t1
const TEMP2: PhysReg = 7; // t2
```

### Tests

Add emission tests at bottom of `emit.rs`:

```rust
#[test]
fn emit_divs32() {
    // Test that DivS32 emits correct div instruction
    // Similar pattern to existing emit tests
}

#[test]
fn emit_icmp_eq() {
    // Test that Icmp32::Eq emits xor + sltiu sequence
}

#[test]
fn emit_icmp_lts() {
    // Test that Icmp32::LtS emits slt
}

#[test]
fn emit_select() {
    // Test that Select32 emits sub + and + add sequence
}
```

## Validate

```bash
cargo test -p lpvm-native -- emit::tests
cargo check -p lpvm-native
```

All emission tests should pass.

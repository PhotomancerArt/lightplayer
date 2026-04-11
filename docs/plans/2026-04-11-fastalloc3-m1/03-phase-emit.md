# Phase 3: VInst→PInst Translation

## Scope

Implement `emit_vinst` — the match that translates each VInst variant to
`Vec<PInst>` using resolved physical registers. This is the same translation
that `rv32::alloc.rs` does today, but using the backward walk's resolved PRegs
instead of inline allocation.

## Code Organization Reminders

- `emit_vinst` lives in `walk.rs` (or a helper module if it gets large)
- Follow `rv32::alloc.rs` as reference for the VInst→PInst mapping
- Tests first, helpers at bottom

## Implementation Details

### `emit_vinst` in `fa_alloc/walk.rs`

This function takes the VInst, the resolved def PRegs, and the resolved use
PRegs, and returns the PInst(s) to emit.

```rust
fn emit_vinst(
    state: &mut WalkState,
    idx: usize,
    vinst: &VInst,
    def_pregs: &[(VReg, PReg)],
    use_pregs: &[PReg],
    vreg_pool: &[VReg],
) -> Result<Vec<PInst>, AllocError> {
    let dst = || def_pregs[0].1;  // first def's PReg
    let src1 = || use_pregs[0];
    let src2 = || use_pregs[1];

    match vinst {
        // Arithmetic: dst = op(src1, src2)
        VInst::Add32 { .. } => Ok(vec![PInst::Add { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Sub32 { .. } => Ok(vec![PInst::Sub { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Mul32 { .. } => Ok(vec![PInst::Mul { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::And32 { .. } => Ok(vec![PInst::And { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Or32 { .. }  => Ok(vec![PInst::Or  { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Xor32 { .. } => Ok(vec![PInst::Xor { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Shl32 { .. } => Ok(vec![PInst::Sll { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::ShrS32 { .. } => Ok(vec![PInst::Sra { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::ShrU32 { .. } => Ok(vec![PInst::Srl { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::DivS32 { .. } => Ok(vec![PInst::Div { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::DivU32 { .. } => Ok(vec![PInst::Divu { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::RemS32 { .. } => Ok(vec![PInst::Rem { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::RemU32 { .. } => Ok(vec![PInst::Remu { dst: dst(), src1: src1(), src2: src2() }]),

        // Unary: dst = op(src)
        VInst::Neg32 { .. }  => Ok(vec![PInst::Neg { dst: dst(), src: src1() }]),
        VInst::Bnot32 { .. } => Ok(vec![PInst::Not { dst: dst(), src: src1() }]),
        VInst::Mov32 { .. }  => {
            if dst() != src1() {
                Ok(vec![PInst::Mv { dst: dst(), src: src1() }])
            } else {
                Ok(vec![])
            }
        }

        // Immediate
        VInst::IConst32 { val, .. } => Ok(vec![PInst::Li { dst: dst(), imm: *val }]),

        // Memory
        VInst::Load32 { offset, .. } => {
            Ok(vec![PInst::Lw { dst: dst(), base: src1(), offset: *offset }])
        }
        VInst::Store32 { offset, .. } => {
            // Store has no def; src = use[0], base = use[1]
            Ok(vec![PInst::Sw { src: src1(), base: src2(), offset: *offset }])
        }
        VInst::SlotAddr { slot, .. } => {
            Ok(vec![PInst::SlotAddr { dst: dst(), slot: *slot }])
        }
        VInst::MemcpyWords { size, .. } => {
            Ok(vec![PInst::MemcpyWords { dst: src1(), src: src2(), size: *size }])
        }

        // Compare — multi-instruction sequences using SCRATCH
        VInst::Icmp32 { cond, .. } => emit_icmp(dst(), src1(), src2(), *cond),
        VInst::IeqImm32 { imm, .. } => emit_ieq_imm(dst(), src1(), *imm),

        // Return
        VInst::Ret { vals, .. } => {
            let mut out = Vec::new();
            for (k, v) in vals.vregs(vreg_pool).iter().enumerate() {
                let src = state.pool.home(*v).unwrap_or(use_pregs[k]);
                let dst_ret = gpr::RET_REGS[k];
                if src != dst_ret {
                    out.push(PInst::Mv { dst: dst_ret, src });
                }
            }
            out.push(PInst::Ret);
            Ok(out)
        }

        VInst::Label(..) => Ok(vec![]),

        _ => Err(AllocError::UnsupportedControlFlow),
    }
}
```

### `emit_icmp` and `emit_ieq_imm` helpers

Copy the Icmp32 and IeqImm32 logic from `rv32::alloc.rs`:

```rust
fn emit_icmp(dst: PReg, lhs: PReg, rhs: PReg, cond: IcmpCond) -> Result<Vec<PInst>, AllocError> {
    let scratch = gpr::SCRATCH;
    match cond {
        IcmpCond::Eq => Ok(vec![
            PInst::Xor { dst: scratch, src1: lhs, src2: rhs },
            PInst::Seqz { dst, src: scratch },
        ]),
        IcmpCond::Ne => Ok(vec![
            PInst::Xor { dst: scratch, src1: lhs, src2: rhs },
            PInst::Snez { dst, src: scratch },
        ]),
        IcmpCond::LtS => Ok(vec![PInst::Slt { dst, src1: lhs, src2: rhs }]),
        IcmpCond::LeS => Ok(vec![
            PInst::Slt { dst: scratch, src1: rhs, src2: lhs },
            PInst::Seqz { dst, src: scratch },
        ]),
        IcmpCond::GtS => Ok(vec![PInst::Slt { dst, src1: rhs, src2: lhs }]),
        IcmpCond::GeS => Ok(vec![
            PInst::Slt { dst: scratch, src1: lhs, src2: rhs },
            PInst::Seqz { dst, src: scratch },
        ]),
        IcmpCond::LtU => Ok(vec![PInst::Sltu { dst, src1: lhs, src2: rhs }]),
        IcmpCond::LeU => Ok(vec![
            PInst::Sltu { dst: scratch, src1: rhs, src2: lhs },
            PInst::Seqz { dst, src: scratch },
        ]),
        IcmpCond::GtU => Ok(vec![PInst::Sltu { dst, src1: rhs, src2: lhs }]),
        IcmpCond::GeU => Ok(vec![
            PInst::Sltu { dst: scratch, src1: lhs, src2: rhs },
            PInst::Seqz { dst, src: scratch },
        ]),
    }
}

fn emit_ieq_imm(dst: PReg, src: PReg, imm: i32) -> Result<Vec<PInst>, AllocError> {
    let scratch = gpr::SCRATCH;
    Ok(vec![
        PInst::Li { dst: scratch, imm },
        PInst::Xor { dst: scratch, src1: src, src2: scratch },
        PInst::Seqz { dst, src: scratch },
    ])
}
```

### Tests

```rust
#[test]
fn emit_add_produces_pinst() {
    // Verify VInst::Add32 with resolved PRegs produces PInst::Add
}

#[test]
fn emit_icmp_eq_uses_scratch() {
    // Verify Icmp32 Eq produces Xor+Seqz using SCRATCH register
}

#[test]
fn emit_ret_moves_to_ret_reg() {
    // Verify Ret with value not in a0 produces Mv then Ret
}

#[test]
fn emit_mov_elided_when_same_reg() {
    // Verify Mov32 where src and dst resolve to same PReg produces nothing
}
```

## Validate

```bash
cargo test -p lpvm-native-fa --lib -- fa_alloc
```

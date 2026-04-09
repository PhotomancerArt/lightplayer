## Scope of Phase

Update `lower.rs` to handle new LPIR ops: division, remainder, comparisons, and selection.

## Code Organization Reminders

- Add new match arms in alphabetical order by LPIR op name
- For Select, generate multiple VInsts using temporary vregs
- Place helper functions at bottom of file
- Add comprehensive tests for each lowered op

## Implementation Details

### Changes to `lower.rs`

Add new match arms to `lower_op()`:

```rust
pub fn lower_op(
    op: &Op,
    float_mode: FloatMode,
    src_op: Option<u32>,
    func: &IrFunction,
) -> Result<VInst, LowerError> {
    match op {
        // ... existing arms ...

        // Division and remainder
        Op::IdivS { dst, lhs, rhs } => Ok(VInst::DivS32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IdivU { dst, lhs, rhs } => Ok(VInst::DivU32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IremS { dst, lhs, rhs } => Ok(VInst::RemS32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),
        Op::IremU { dst, lhs, rhs } => Ok(VInst::RemU32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            src_op,
        }),

        // Comparisons - map to Icmp32 with appropriate condition
        Op::Ieq { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::Eq,
            src_op,
        }),
        Op::Ine { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::Ne,
            src_op,
        }),
        Op::IltS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LtS,
            src_op,
        }),
        Op::IleS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LeS,
            src_op,
        }),
        Op::IgtS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GtS,
            src_op,
        }),
        Op::IgeS { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GeS,
            src_op,
        }),
        Op::IltU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LtU,
            src_op,
        }),
        Op::IleU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::LeU,
            src_op,
        }),
        Op::IgtU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GtU,
            src_op,
        }),
        Op::IgeU { dst, lhs, rhs } => Ok(VInst::Icmp32 {
            dst: *dst,
            lhs: *lhs,
            rhs: *rhs,
            cond: IcmpCond::GeU,
            src_op,
        }),

        // Selection - expand to branchless arithmetic sequence
        // Note: This requires vreg allocation for temporaries
        Op::Select { dst, cond, if_true, if_false } => {
            // Generate: result = false + ((true - false) & cond)
            // We need two temporary vregs
            let tmp1 = func.alloc_vreg(IrType::I32);
            let tmp2 = func.alloc_vreg(IrType::I32);

            // Return a list of VInsts - requires changing return type
            // For now, we emit as a pseudo-instruction that emit.rs expands
            Ok(VInst::Select32 {
                dst: *dst,
                cond: *cond,
                if_true: *if_true,
                if_false: *if_false,
                src_op,
            })
        }
    }
}
```

**Alternative approach for Select**: Since we can't easily return multiple VInsts from `lower_op`, we could:

1. Add a `Select32` VInst that gets expanded during emission (simpler)
2. Change `lower_op` to return `Vec<VInst>` (more invasive)

Let's go with option 1 - add `Select32` VInst:

```rust
// In vinst.rs
Select32 {
    dst: VReg,
    cond: VReg,
    if_true: VReg,
    if_false: VReg,
    src_op: Option<u32>,
},
```

Then in emission, expand it to the arithmetic sequence.

### Tests

Add tests at bottom of `lower.rs`:

```rust
#[test]
fn lower_idivs() {
    let op = Op::IdivS {
        dst: v(2),
        lhs: v(0),
        rhs: v(1),
    };
    let f = empty_func();
    let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
    assert!(matches!(got, VInst::DivS32 { .. }));
}

#[test]
fn lower_ieq() {
    let op = Op::Ieq {
        dst: v(2),
        lhs: v(0),
        rhs: v(1),
    };
    let f = empty_func();
    let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
    match got {
        VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::Eq),
        _ => panic!("expected Icmp32"),
    }
}

#[test]
fn lower_iltu() {
    let op = Op::IltU {
        dst: v(2),
        lhs: v(0),
        rhs: v(1),
    };
    let f = empty_func();
    let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
    match got {
        VInst::Icmp32 { cond, .. } => assert_eq!(cond, IcmpCond::LtU),
        _ => panic!("expected Icmp32"),
    }
}

#[test]
fn lower_select() {
    let op = Op::Select {
        dst: v(3),
        cond: v(0),
        if_true: v(1),
        if_false: v(2),
    };
    let f = empty_func();
    let got = lower_op(&op, FloatMode::Q32, Some(0), &f).expect("ok");
    assert!(matches!(got, VInst::Select32 { .. }));
}
```

## Validate

```bash
cargo test -p lpvm-native -- lower::tests
cargo check -p lpvm-native
```

All new lowering tests should pass.
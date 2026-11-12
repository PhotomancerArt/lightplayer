## Scope of Phase

Add new VInst variants for core integer operations: division, remainder, comparison, and bitwise AND (needed for Select expansion).

## Code Organization Reminders

- Add new VInst variants to the enum in alphabetical/logical order
- Include `src_op` field for debug line tracking
- Update `defs()` and `uses()` methods for new variants
- Place helper functions at bottom of file

## Implementation Details

### Changes to `vinst.rs`

Add new VInst variants:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    // ... existing variants ...

    // Bitwise (needed for Select expansion)
    And32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,
    },

    // Division and remainder
    DivS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    DivU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    RemS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },
    RemU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: Option<u32>,
    },

    // Comparison
    Icmp32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
        src_op: Option<u32>,
    },
}

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

Update `src_op()` method to handle new variants.

Update `defs()` to include new destination registers:

```rust
pub fn defs(&self) -> impl Iterator<Item = VReg> + '_ {
    let mut v = Vec::new();
    match self {
        // ... existing ...
        VInst::And32 { dst, .. }
        | VInst::DivS32 { dst, .. }
        | VInst::DivU32 { dst, .. }
        | VInst::RemS32 { dst, .. }
        | VInst::RemU32 { dst, .. }
        | VInst::Icmp32 { dst, .. } => v.push(*dst),
        // ...
    }
    v.into_iter()
}
```

Update `uses()` to include source registers:

```rust
pub fn uses(&self) -> impl Iterator<Item = VReg> + '_ {
    let mut v = Vec::new();
    match self {
        // ... existing ...
        VInst::And32 { src1, src2, .. }
        | VInst::DivS32 { lhs: src1, rhs: src2, .. }
        | VInst::DivU32 { lhs: src1, rhs: src2, .. }
        | VInst::RemS32 { lhs: src1, rhs: src2, .. }
        | VInst::RemU32 { lhs: src1, rhs: src2, .. }
        | VInst::Icmp32 { lhs: src1, rhs: src2, .. } => {
            v.push(*src1);
            v.push(*src2);
        }
        // ...
    }
    v.into_iter()
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native -- vinst
```

No new tests needed yet - just ensure it compiles.

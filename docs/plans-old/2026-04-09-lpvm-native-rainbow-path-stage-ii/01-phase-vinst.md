# Phase 1: Add VInst Variants

## Scope of Phase

Add `Br` and `BrIf` VInst variants to represent control flow operations.

## Code Organization Reminders

- Place new variants after `Select32`, before `Mov32`
- Update `src_op()`, `defs()`, and `uses()` match arms
- Keep related functionality grouped together

## Implementation Details

### 1. Add to `vinst.rs`

Add these variants to the `VInst` enum:

```rust
/// Unconditional branch to label.
Br {
    target: LabelId,
    src_op: Option<u32>,
},

/// Conditional branch: if invert=false, branch when cond != 0;
/// if invert=true, branch when cond == 0.
BrIf {
    cond: VReg,
    target: LabelId,
    invert: bool,
    src_op: Option<u32>,
},
```

### 2. Update `src_op()` method

Add to the match:
```rust
| VInst::Br { src_op, .. }
| VInst::BrIf { src_op, .. }
```

### 3. Update `defs()` method

`Br` and `BrIf` don't define any registers (they only read). No change needed to `defs()`.

### 4. Update `uses()` method

Add a match arm:
```rust
VInst::Br { .. } => {}
VInst::BrIf { cond, .. } => v.push(*cond),
```

### 5. Update `is_call()` method

Branches are not calls. No change needed.

## Tests

Add unit tests in `vinst.rs` (in the `mod tests` section):

```rust
#[test]
fn br_src_op_roundtrips() {
    let inst = VInst::Br { target: 5, src_op: Some(3) };
    assert_eq!(inst.src_op(), Some(3));
}

#[test]
fn brif_uses_cond() {
    let inst = VInst::BrIf { cond: VReg(1), target: 2, invert: true, src_op: None };
    let uses: Vec<_> = inst.uses().collect();
    assert_eq!(uses, vec![VReg(1)]);
}

#[test]
fn brif_defs_empty() {
    let inst = VInst::BrIf { cond: VReg(1), target: 2, invert: false, src_op: None };
    let defs: Vec<_> = inst.defs().collect();
    assert!(defs.is_empty());
}
```

## Validate

```bash
cargo test -p lpvm-native
```

Expected: All existing tests pass, new tests pass.

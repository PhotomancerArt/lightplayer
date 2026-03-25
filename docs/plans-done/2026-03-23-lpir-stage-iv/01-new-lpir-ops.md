# Phase 1: Add New LPIR Ops

## Scope

Add 8 new float math ops to the `lpir` crate: `Fabs`, `Fsqrt`, `Fmin`,
`Fmax`, `Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`. These correspond to
instructions that both Cranelift and WASM have natively. Update all
downstream code: interpreter, printer, parser, validator, and tests.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### `op.rs` — add 8 new variants

Add after the `Fneg` variant (in the float arithmetic section):

```rust
// --- Float math ---
Fabs { dst: VReg, src: VReg },
Fsqrt { dst: VReg, src: VReg },
Fmin { dst: VReg, lhs: VReg, rhs: VReg },
Fmax { dst: VReg, lhs: VReg, rhs: VReg },
Ffloor { dst: VReg, src: VReg },
Fceil { dst: VReg, src: VReg },
Ftrunc { dst: VReg, src: VReg },
Fnearest { dst: VReg, src: VReg },
```

### `interp.rs` — interpreter semantics

Add to the `eval_op` match, after the `Fneg` arm:

```rust
Op::Fabs { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(a.abs()))?;
}
Op::Fsqrt { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(a.sqrt()))?;
}
Op::Fmin { dst, lhs, rhs } => {
    let a = val_f32(get_reg(regs, *lhs)?)?;
    let b = val_f32(get_reg(regs, *rhs)?)?;
    set_reg(regs, *dst, Value::F32(a.min(b)))?;
}
Op::Fmax { dst, lhs, rhs } => {
    let a = val_f32(get_reg(regs, *lhs)?)?;
    let b = val_f32(get_reg(regs, *rhs)?)?;
    set_reg(regs, *dst, Value::F32(a.max(b)))?;
}
Op::Ffloor { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(a.floor()))?;
}
Op::Fceil { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(a.ceil()))?;
}
Op::Ftrunc { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(a.trunc()))?;
}
Op::Fnearest { dst, src } => {
    let a = val_f32(get_reg(regs, *src)?)?;
    set_reg(regs, *dst, Value::F32(round_even(a)))?;
}
```

Add a `round_even` helper at the bottom of `interp.rs`:
```rust
fn round_even(v: f32) -> f32 {
    let r = v.round();
    if (v - r).abs() == 0.5 {
        let f = r as i64;
        if f % 2 != 0 { r - v.signum() } else { r }
    } else {
        r
    }
}
```

Note: `f32::min`/`f32::max` use IEEE 754-2008 minNum/maxNum semantics
(propagate non-NaN), which matches WASM `f32.min`/`f32.max` behavior.

### `print.rs` — text output

Add to `print_simple_op`, using the existing `unary` and `bin_int_cmp`
helpers:

```rust
Op::Fabs { dst, src } => unary(out, st, ind, "fabs", *dst, *src),
Op::Fsqrt { dst, src } => unary(out, st, ind, "fsqrt", *dst, *src),
Op::Fmin { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fmin", *dst, *lhs, *rhs),
Op::Fmax { dst, lhs, rhs } => bin_int_cmp(out, st, ind, "fmax", *dst, *lhs, *rhs),
Op::Ffloor { dst, src } => unary(out, st, ind, "ffloor", *dst, *src),
Op::Fceil { dst, src } => unary(out, st, ind, "fceil", *dst, *src),
Op::Ftrunc { dst, src } => unary(out, st, ind, "ftrunc", *dst, *src),
Op::Fnearest { dst, src } => unary(out, st, ind, "fnearest", *dst, *src),
```

### `parse.rs` — text input

Add the new names to the opcode parsing logic (where other unary/binary
ops are parsed). Unary ops: `fabs`, `fsqrt`, `ffloor`, `fceil`, `ftrunc`,
`fnearest`. Binary ops: `fmin`, `fmax`.

Follow the pattern of existing ops like `fneg` (unary) and `fadd` (binary).

### `validate.rs` — type checking

Add the new ops to the validator. Unary float ops (`Fabs`, `Fsqrt`,
`Ffloor`, `Fceil`, `Ftrunc`, `Fnearest`): dst and src must be `F32`.
Binary float ops (`Fmin`, `Fmax`): dst, lhs, rhs must be `F32`.

Follow the existing pattern used for `Fneg` (unary) and `Fadd` (binary).

### Tests

Add to `tests/interp.rs`:

- `interp_fabs`: test positive, negative, zero, NaN
- `interp_fsqrt`: test perfect squares, zero, negative (→ NaN)
- `interp_fmin_fmax`: test normal values, NaN propagation
- `interp_ffloor_fceil_ftrunc`: test positive/negative with fractional parts
- `interp_fnearest`: test ties-to-even behavior (0.5→0, 1.5→2, 2.5→2)

Add to `tests/all_ops_roundtrip.rs`: include the new ops in the
roundtrip module (they will be included automatically if the roundtrip
test covers all ops).

Update the `Op` size assertion test if it exists (the enum gets larger).

## Validate

```
cargo test -p lpir
cargo +nightly fmt -p lpir -- --check
cargo clippy -p lpir
```

All existing tests must pass. New tests must pass.

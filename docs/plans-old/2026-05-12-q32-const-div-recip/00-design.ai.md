# Q32 Const-Div Fast Path Design

## Scope

This plan adds an explicit LPIR representation for float division by a compile-time constant and uses it to make Q32 constant division fast in the native JIT path.

The product direction is fast-only normal rendering. Old accurate/saturating mode machinery should be treated as transitional; preserve it only where removing it would distract from the const-div implementation.

## File Structure

```text
lp-shader/
  lpir/
    src/
      lpir_op.rs
      format.rs
      parse.rs
      validate.rs
      tests/
        all_ops_roundtrip.rs
        interp.rs
        validate.rs
  lps-frontend/
    src/
      lower_binary.rs
      lower_expr.rs
      naga_util.rs
    tests/
      ...
  lpvm-native/
    src/
      lower.rs
      compile.rs
  lpvm-wasm/
    src/emit/
      ops.rs
      q32.rs
  lps-filetests/
    filetests/scalar/float/
      q32fast-div-const.glsl
      q32fast-div-recip.glsl
      op-divide.glsl
lp-core/
  lpc-model/src/nodes/shader/
    glsl_opts.rs
    shader_def.rs
  lpc-engine/src/nodes/shader/
    shader_node.rs
```

## Architecture Summary

Add a semantic LPIR operation:

```rust
LpirOp::FdivConstF32 {
    dst: VReg,
    lhs: VReg,
    rhs: f32,
}
```

This operation means "float division by a compile-time constant." It does not encode the Q32 reciprocal algorithm and it does not imply exact division. The backend owns lowering according to target and float mode.

The frontend emits `FdivConstF32` directly when it is lowering `BinaryOperator::Divide` and the RHS is a cheap compile-time float constant. This is intentionally pragmatic: it catches the common shader source forms without building a broad middle-end constant-propagation pass.

The native Q32 backend lowers nonzero `FdivConstF32` as multiply by a precomputed Q32 reciprocal:

```text
recip_q32 = q32(1.0 / rhs)
dst = wrapping_q32_mul(lhs, recip_q32)
```

This is intentionally faster and less exact than preserving the reciprocal helper's edge behavior. The normal product path favors FPS; later debug probes will cover correctness/range diagnostics.

Constant zero divisors are the exception. The backend should not emit multiply by a non-finite reciprocal. For `rhs == 0.0`, materialize zero and fall back to existing dynamic `Fdiv` helper behavior.

## Main Components

### LPIR Op

`FdivConstF32` must participate in the ordinary LPIR plumbing:

- `def_vreg`
- used vregs
- display/format
- parser
- validator
- all-ops roundtrip tests
- interpreter behavior if the interpreter supports float operations under the relevant mode

Validation should require:

- `dst` has `IrType::F32`
- `lhs` has `IrType::F32`
- `rhs` is finite enough for the chosen semantics, or parser/validator documents accepted values

### Frontend Emission

`lps-frontend::lower_binary_vec` receives the RHS expression handle before RHS is lowered. Add a helper that extracts float constants from:

- `Expression::Literal(Literal::F32)`
- `Expression::Literal(Literal::F64)` converted to `f32`
- `Expression::Constant` whose initializer resolves to a float literal/global constant
- vector constants where each lane is known, if Naga represents them in an accessible way

For scalar RHS broadcast, emit `FdivConstF32` for each left lane. For vector RHS constants, emit one op per lane with that lane's constant.

If extraction fails, keep the existing dynamic `Fdiv` path.

### Native Lowering

In `lpvm-native::lower`, add `FdivConstF32` lowering.

For Q32:

- If `rhs == 0.0`, materialize a Q32 zero temporary and reuse/factor the existing dynamic `Fdiv` lowering.
- Otherwise:
  - convert `rhs` to Q32 the same way native `FconstF32` does or via a shared helper introduced during implementation
  - compute the reciprocal Q32 constant from `1.0 / rhs`
  - materialize reciprocal into a temp `IConst32`
  - emit the existing inline wrapping Q32 multiply sequence with `lhs` and reciprocal temp

For non-Q32:

- Prefer conservative lowering: materialize the constant and use existing `Fdiv`.
- Keep the implementation small unless F32 mode needs a direct optimization later.

### Wasm Lowering

Wasm must support the new op so host paths still compile.

Acceptable first implementation:

- materialize `rhs` and reuse existing `Fdiv` lowering logic, or
- for Q32, emit `lhs * q32(1.0 / rhs)` if there is an existing wrapping multiply helper that makes this straightforward.

Do not let wasm parity block native JIT perf work unless tests require exact host/device agreement for this op.

### Mode Cleanup

The product goal is to remove the fast/accurate math split from normal rendering. This plan should reduce reliance on mode switches, but it does not need to fully delete every type and old filetest knob in the same pass.

Recommended cleanup boundary:

- Hardcode native Q32 normal operations to fast behavior where touched.
- Stop adding new tests or code branches for saturating/reference normal mode.
- If deleting public model slots or `compile-opt(q32.*)` causes broad churn, defer that deletion to a separate cleanup plan.

## Phase Plan

| Phase | Title | parallel | sub-agent |
| --- | --- | --- | --- |
| 01 | LPIR const-div op | - | main |
| 02 | Frontend const RHS emission | 03 after LPIR API stabilizes | supervised |
| 03 | Native and wasm lowering | 02 after LPIR API stabilizes | main |
| 04 | Filetests and perf comparison | - | supervised |
| 05 | Fast-only cleanup and final validation | - | main |

Phases 02 and 03 can overlap only after Phase 01 has landed because both depend on the new LPIR operation. They should not edit the same files except for mechanical test updates coordinated through Phase 01.

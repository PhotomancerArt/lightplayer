# Phase 4: Component-Wise Math

## Scope

Extend `lower_math.rs` to handle vector arguments. Most GLSL math
functions apply per-component to vectors (`sin(vec3)` = 3 `sin` calls).
Some functions have special vector semantics (`dot`, `cross`, `length`,
`normalize`, `reflect`, `refract`, `faceForward`, `distance`).

## Implementation Details

### `lower_math.rs` — detection and dispatch

Change the top-level `lower_math` to accept and return `VRegVec`:

```rust
pub(crate) fn lower_math_vec(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    arg: Handle<naga::Expression>,
    arg1: Option<Handle<naga::Expression>>,
    arg2: Option<Handle<naga::Expression>>,
    arg3: Option<Handle<naga::Expression>>,
) -> Result<VRegVec, LowerError>
```

Determine argument width from `expr_type_inner`. For width 1, delegate
to existing scalar `lower_math` wrapped in a 1-element SmallVec.

For width > 1, check if the function is vector-specific or per-component.

### Per-component math functions

These apply the scalar implementation to each component independently:

`Abs`, `Sqrt`, `Floor`, `Ceil`, `Round`, `Trunc`, `Min`, `Max`, `Mix`,
`SmoothStep`, `Step`, `Fma`, `Clamp`, `Sign`, `Fract`, `InverseSqrt`,
`Saturate`, `Radians`, `Degrees`, `Sin`, `Cos`, `Tan`, `Asin`, `Acos`,
`Atan`, `Atan2`, `Sinh`, `Cosh`, `Tanh`, `Asinh`, `Acosh`, `Atanh`,
`Exp`, `Exp2`, `Log`, `Log2`, `Pow`, `Ldexp`

Pattern:

```rust
fn lower_math_per_component(
    ctx: &mut LowerCtx<'_>,
    fun: MathFunction,
    args: &[Handle<naga::Expression>],
) -> Result<VRegVec, LowerError> {
    let n = naga_type_width(expr_type_inner(ctx.module, ctx.func, args[0])?);
    let arg_vecs: Vec<VRegVec> = args.iter()
        .map(|a| lower_expr_vec(ctx, *a))
        .collect::<Result<_, _>>()?;

    let mut result = VRegVec::new();
    for i in 0..n {
        // For each component, build scalar args (broadcast if needed)
        let scalar_args: Vec<VReg> = arg_vecs.iter()
            .map(|vs| vs[i.min(vs.len() - 1)])
            .collect();
        let v = lower_math_scalar_component(ctx, fun, &scalar_args)?;
        result.push(v);
    }
    Ok(result)
}
```

`lower_math_scalar_component` dispatches to the existing scalar math
functions but takes pre-lowered VRegs instead of expression handles.
This requires refactoring the existing scalar math functions to accept
VRegs directly (rather than expression handles) — extract the arithmetic
logic from the expression-lowering logic.

### Scalar broadcast in multi-arg functions

`mix(vec3, vec3, float)` — the third argument is scalar, broadcast to
each component. The `i.min(vs.len() - 1)` pattern handles this: if
`arg_vecs[2]` has length 1, index 0 is reused for all N iterations.

Similarly: `step(float, vec3)`, `smoothstep(float, float, vec3)`,
`clamp(vec3, float, float)`.

### Vector-specific math functions

These have special semantics and do NOT apply per-component:

#### `Dot` → scalar result

```rust
MathFunction::Dot => {
    let a = lower_expr_vec(ctx, arg)?;
    let b = lower_expr_vec(ctx, arg1.unwrap())?;
    let dot = emit_dot_product(ctx, &a, &b)?;
    Ok(smallvec![dot])
}
```

`emit_dot_product`: multiply corresponding components, sum all products.

```rust
fn emit_dot_product(ctx: &mut LowerCtx<'_>, a: &[VReg], b: &[VReg]) -> Result<VReg, LowerError> {
    assert_eq!(a.len(), b.len());
    let mut sum = {
        let d = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul { dst: d, lhs: a[0], rhs: b[0] });
        d
    };
    for i in 1..a.len() {
        let prod = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fmul { dst: prod, lhs: a[i], rhs: b[i] });
        let next = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(Op::Fadd { dst: next, lhs: sum, rhs: prod });
        sum = next;
    }
    Ok(sum)
}
```

#### `Cross` → vec3 result

Only valid for vec3:

```rust
// cross(a, b) = (a.y*b.z - a.z*b.y, a.z*b.x - a.x*b.z, a.x*b.y - a.y*b.x)
fn emit_cross(ctx: &mut LowerCtx<'_>, a: &[VReg], b: &[VReg]) -> Result<VRegVec, LowerError> {
    let x = emit_fsub_fmul_pair(ctx, a[1], b[2], a[2], b[1])?;
    let y = emit_fsub_fmul_pair(ctx, a[2], b[0], a[0], b[2])?;
    let z = emit_fsub_fmul_pair(ctx, a[0], b[1], a[1], b[0])?;
    Ok(smallvec![x, y, z])
}

fn emit_fsub_fmul_pair(ctx, a1, b1, a2, b2) -> VReg {
    let p1 = fmul(ctx, a1, b1);
    let p2 = fmul(ctx, a2, b2);
    fsub(ctx, p1, p2)
}
```

#### `Length` → scalar result

```rust
// length(v) = sqrt(dot(v, v))
fn emit_length(ctx, v: &[VReg]) -> Result<VReg, LowerError> {
    let d = emit_dot_product(ctx, v, v)?;
    let r = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(Op::Fsqrt { dst: r, src: d });
    Ok(r)
}
```

#### `Distance` → scalar result

```rust
// distance(a, b) = length(a - b)
```

Subtract component-wise, then `emit_length`.

#### `Normalize` → vector result

```rust
// normalize(v) = v / length(v)
```

Compute length, divide each component.

#### `FaceForward` → vector result

```rust
// faceForward(n, i, nref) = dot(nref, i) < 0 ? n : -n
```

#### `Reflect` → vector result

```rust
// reflect(i, n) = i - 2.0 * dot(n, i) * n
```

#### `Refract` → vector result

```rust
// refract(i, n, eta):
//   k = 1.0 - eta*eta * (1.0 - dot(n,i)*dot(n,i))
//   if k < 0: return zero vector
//   else: return eta*i - (eta*dot(n,i) + sqrt(k)) * n
```

### Existing scalar math refactoring

The existing `lower_mix`, `lower_smoothstep`, `lower_step`, etc. take
expression handles and call `ctx.ensure_expr`. For per-component use,
we need versions that take pre-lowered VRegs. Two approaches:

**A) Refactor to VReg-based helpers**: Create `_from_vregs` variants of
each function. The original functions call `ensure_expr` then delegate.

**B) Use synthetic expressions**: Not feasible with Naga's arena.

Go with A. For each existing scalar math function that needs vectorization,
create a `_vregs` variant:

```rust
fn lower_mix_vregs(ctx: &mut LowerCtx<'_>, x: VReg, y: VReg, t: VReg) -> Result<VReg, LowerError>
fn lower_clamp_vregs(ctx: &mut LowerCtx<'_>, x: VReg, lo: VReg, hi: VReg, k: ScalarKind) -> ...
// etc.
```

## Validate

```
cargo test -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
cargo clippy -p lp-glsl-naga
```

Vector math filetests (dot, cross, normalize, mix on vectors, etc.)
should now lower.

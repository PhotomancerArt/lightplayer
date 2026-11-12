# Phase 5: User Function Calls + Math Builtins

## Scope

Implement user function calls (`Statement::Call`), math builtin lowering
(`Expression::Math`), and `Expression::CallResult`. This phase introduces
the three-tiered math strategy from the design doc.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.

## Implementation Details

### User function calls — `lower_stmt.rs`

In the `Statement::Call` match arm:

1. Check if the callee is an LPFX function → delegate to Phase 6 (stub
   for now with `todo!()` behind an `if name.starts_with("lpfn_")` guard).
2. Check if the callee is a math-only function (no body — these are Naga
   internal stubs). If so, skip (math is handled at expression level).
3. Otherwise, this is a user function call:
    - Lower each argument via `ctx.ensure_expr(arg)`.
    - Look up the `CalleeRef` from `ctx.func_map[function_handle]`.
    - Determine result VRegs:
        - If `result` is `Some(expr_handle)`: allocate a VReg for the result
          type, cache it in `expr_cache[expr_handle]`.
        - If `result` is `None`: no result VRegs.
    - `ctx.fb.push_call(callee_ref, &arg_vregs, &result_vregs)`

### `Expression::CallResult` — `lower_expr.rs`

When an expression is `CallResult(function_call_handle)`:

- The VReg was already cached when the `Statement::Call` was processed
  (result VReg allocated and stored in expr_cache).
- Return from cache. If not in cache, return `LowerError::Internal`
  ("CallResult before Call").

### Math builtins — `lower_math.rs`

```rust
pub(crate) fn lower_math(
    ctx: &mut LowerCtx,
    fun: naga::MathFunction,
    args: &[Handle<Expression>],
) -> Result<VReg, LowerError>
```

Called from `lower_expr.rs` when the expression is `Expression::Math`.

#### Tier 1 — Direct LPIR ops

| Naga `MathFunction` | LPIR Op                                 |
|---------------------|-----------------------------------------|
| `Abs` (float)       | `Fabs`                                  |
| `Abs` (int)         | inline: `Ineg` + `Select` (if negative) |
| `Sqrt`              | `Fsqrt`                                 |
| `Floor`             | `Ffloor`                                |
| `Ceil`              | `Fceil`                                 |
| `Round`             | `Fnearest`                              |
| `Trunc`             | `Ftrunc`                                |
| `Min` (float)       | `Fmin`                                  |
| `Min` (int)         | `IltS`/`IltU` + `Select`                |
| `Max` (float)       | `Fmax`                                  |
| `Max` (int)         | `IgtS`/`IgtU` + `Select`                |

#### Tier 2 — Inline decomposition

**`Mix(x, y, t)`**: (float only)

```
d = fsub(y, x)
m = fmul(d, t)
r = fadd(x, m)
```

**`SmoothStep(e0, e1, x)`**:

```
range = fsub(e1, e0)
raw   = fsub(x, e0)
div   = fdiv(raw, range)
lo    = fmax(div, 0.0)
t     = fmin(lo, 1.0)
two   = fconst 2.0
three = fconst 3.0
t2    = fmul(t, t)
twot  = fmul(two, t)
diff  = fsub(three, twot)
r     = fmul(t2, diff)
```

**`Step(edge, x)`**:

```
cmp = fge(x, edge)
one = fconst 1.0
zero = fconst 0.0
r = select(cmp, one, zero)
```

**`Fma(a, b, c)`**:

```
m = fmul(a, b)
r = fadd(m, c)
```

**`Clamp(x, lo, hi)`** (float):

```
t = fmax(x, lo)
r = fmin(t, hi)
```

**`Clamp(x, lo, hi)`** (int):

```
Use comparison + select chains (signed or unsigned)
```

**`Sign(x)`** (float):

```
zero = fconst 0.0
one = fconst 1.0
neg1 = fconst -1.0
gt = fgt(x, zero)
lt = flt(x, zero)
r1 = select(gt, one, zero)
r  = select(lt, neg1, r1)
```

**`Fract(x)`**:

```
fl = ffloor(x)
r  = fsub(x, fl)
```

**`Mod(x, y)`** (same as float binary Modulo):

```
d  = fdiv(x, y)
fl = ffloor(d)
m  = fmul(fl, y)
r  = fsub(x, m)
```

**`InverseSqrt(x)`**: tier 3 import (or inline: `1/sqrt(x)`)
Actually, inline is fine:

```
sq = fsqrt(x)
one = fconst 1.0
r = fdiv(one, sq)
```

#### Tier 3 — Import calls (`@std.math::name`)

For each of these, create an `ImportDecl` with `module_name: "std.math"`,
`func_name: <name>`, `param_types: [F32; N]`, `return_types: [F32]`.

| MathFunction | Import name | Params       |
|--------------|-------------|--------------|
| Sin          | sin         | 1            |
| Cos          | cos         | 1            |
| Tan          | tan         | 1            |
| Asin         | asin        | 1            |
| Acos         | acos        | 1            |
| Atan         | atan        | 1            |
| Atan2        | atan2       | 2            |
| Sinh         | sinh        | 1            |
| Cosh         | cosh        | 1            |
| Tanh         | tanh        | 1            |
| Asinh        | asinh       | 1            |
| Acosh        | acosh       | 1            |
| Atanh        | atanh       | 1            |
| Exp          | exp         | 1            |
| Exp2         | exp2        | 1            |
| Log          | log         | 1            |
| Log2         | log2        | 1            |
| Pow          | pow         | 2            |
| Ldexp        | ldexp       | 2 (F32, I32) |

Import deduplication: use `ctx.import_map` to track already-created
imports. Key is `"std.math::sin"` etc. If the import already exists,
reuse its `CalleeRef`.

Calling convention:

- Lower all math args via `ctx.ensure_expr`.
- Allocate result VReg (F32).
- `ctx.fb.push_call(callee_ref, &arg_vregs, &[result_vreg])`
- Return `result_vreg`.

### `std_math_handler.rs` — test handler

```rust
pub struct StdMathHandler;

impl lpir::interp::ImportHandler for StdMathHandler {
    fn call(&mut self, module_name: &str, func_name: &str, args: &[Value])
        -> Result<Vec<Value>, InterpError>
    {
        if module_name != "std.math" {
            return Err(InterpError::Import(format!("unknown module {module_name}")));
        }
        let result = match func_name {
            "sin" => args[0].as_f32().unwrap().sin(),
            "cos" => args[0].as_f32().unwrap().cos(),
            // ... etc for all tier 3 functions ...
            _ => return Err(InterpError::Import(format!("unknown func {func_name}"))),
        };
        Ok(vec![Value::F32(result)])
    }
}
```

## Validate

```
cargo check -p lps-frontend
cargo +nightly fmt -p lps-frontend -- --check
```

After this phase, GLSL programs with user function calls and math
builtins (sin, cos, mix, smoothstep, etc.) lower to complete LPIR
(minus LPFX).

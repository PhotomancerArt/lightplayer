# Phase 4: MathCall Builtins and GLSL Mapping

## Scope

Write two spec chapters:
- `docs/lpir/06-mathcall.md` — MathFunc enumeration, mathcall op
  semantics, backend expectations.
- `docs/lpir/08-glsl-mapping.md` — Full mapping from Naga expressions
  and statements to LPIR ops, including vector scalarization patterns.

## Reminders

- This is a spec-writing phase, no Rust code.
- Enumerate every MathFunc with its signature and GLSL source.
- The mapping table should cover every Naga expression and statement
  variant we handle today.

## Implementation details

### 1. MathCall Mechanism section

Document the design:
- `mathcall` is a single Op variant: `Op::MathCall { dst, func, args }`.
- `func` is a `MathFunc` enum value.
- New builtins extend `MathFunc` without changing `Op`.
- Inspired by SPIR-V's `OpExtInst` + `GLSL.std.450`.

Syntax:
```
v5:f32 = mathcall fmin(v3, v4)
v6:f32 = mathcall fabs(v3)
v7:f32 = mathcall fsmoothstep(v0, v1, v2)
```

### 2. MathFunc Enumeration

For each MathFunc, document:
- **Name**: text format name
- **Signature**: operand types → result type
- **GLSL equivalent**: the GLSL function it corresponds to
- **Semantics**: brief description (can reference GLSL spec for full details)

#### Float math (unary)

| MathFunc | Signature | GLSL | Description |
|---|---|---|---|
| `fabs` | (f32) → f32 | `abs(x)` | Absolute value |
| `fsign` | (f32) → f32 | `sign(x)` | Sign: -1.0, 0.0, or 1.0 |
| `fround` | (f32) → f32 | `round(x)` | Round to nearest integer |
| `froundeven` | (f32) → f32 | `roundEven(x)` | Round to nearest even |
| `ffloor` | (f32) → f32 | `floor(x)` | Floor |
| `fceil` | (f32) → f32 | `ceil(x)` | Ceiling |
| `ftrunc` | (f32) → f32 | `trunc(x)` | Truncate toward zero |
| `ffract` | (f32) → f32 | `fract(x)` | Fractional part: x - floor(x) |
| `fsqrt` | (f32) → f32 | `sqrt(x)` | Square root |
| `finversesqrt` | (f32) → f32 | `inversesqrt(x)` | 1 / sqrt(x) |
| `fsin` | (f32) → f32 | `sin(x)` | Sine (radians) |
| `fcos` | (f32) → f32 | `cos(x)` | Cosine (radians) |
| `ftan` | (f32) → f32 | `tan(x)` | Tangent (radians) |
| `fasin` | (f32) → f32 | `asin(x)` | Arc sine |
| `facos` | (f32) → f32 | `acos(x)` | Arc cosine |
| `fatan` | (f32) → f32 | `atan(x)` | Arc tangent |
| `fsinh` | (f32) → f32 | `sinh(x)` | Hyperbolic sine |
| `fcosh` | (f32) → f32 | `cosh(x)` | Hyperbolic cosine |
| `ftanh` | (f32) → f32 | `tanh(x)` | Hyperbolic tangent |
| `fasinh` | (f32) → f32 | `asinh(x)` | Inverse hyperbolic sine |
| `facosh` | (f32) → f32 | `acosh(x)` | Inverse hyperbolic cosine |
| `fatanh` | (f32) → f32 | `atanh(x)` | Inverse hyperbolic tangent |
| `fexp` | (f32) → f32 | `exp(x)` | e^x |
| `fexp2` | (f32) → f32 | `exp2(x)` | 2^x |
| `flog` | (f32) → f32 | `log(x)` | Natural log |
| `flog2` | (f32) → f32 | `log2(x)` | Base-2 log |

#### Float math (binary)

| MathFunc | Signature | GLSL | Description |
|---|---|---|---|
| `fmin` | (f32, f32) → f32 | `min(x, y)` | Minimum |
| `fmax` | (f32, f32) → f32 | `max(x, y)` | Maximum |
| `fpow` | (f32, f32) → f32 | `pow(x, y)` | x^y |
| `fatan2` | (f32, f32) → f32 | `atan(y, x)` | Two-argument arc tangent |
| `fstep` | (f32, f32) → f32 | `step(edge, x)` | 0.0 if x < edge, else 1.0 |
| `fldexp` | (f32, i32) → f32 | `ldexp(x, exp)` | x * 2^exp |

#### Float math (ternary)

| MathFunc | Signature | GLSL | Description |
|---|---|---|---|
| `fmix` | (f32, f32, f32) → f32 | `mix(x, y, a)` | Linear interpolation: x*(1-a) + y*a |
| `fclamp` | (f32, f32, f32) → f32 | `clamp(x, min, max)` | Clamp to range |
| `fsmoothstep` | (f32, f32, f32) → f32 | `smoothstep(e0, e1, x)` | Hermite interpolation |
| `ffma` | (f32, f32, f32) → f32 | `fma(a, b, c)` | Fused multiply-add: a*b + c |

#### Integer math

| MathFunc | Signature | GLSL | Description |
|---|---|---|---|
| `iabs_s` | (i32) → i32 | `abs(x)` | Absolute value (signed) |
| `imin_s` | (i32, i32) → i32 | `min(x, y)` | Minimum (signed) |
| `imax_s` | (i32, i32) → i32 | `max(x, y)` | Maximum (signed) |
| `imin_u` | (i32, i32) → i32 | `min(x, y)` | Minimum (unsigned) |
| `imax_u` | (i32, i32) → i32 | `max(x, y)` | Maximum (unsigned) |
| `iclamp_s` | (i32, i32, i32) → i32 | `clamp(x, min, max)` | Clamp (signed) |
| `iclamp_u` | (i32, i32, i32) → i32 | `clamp(x, min, max)` | Clamp (unsigned) |

Note: this is the initial set. The `MathFunc` enum is designed to grow as
we add support for more GLSL builtins. The spec should note which MathFuncs
are required for the current scalar filetests vs which are included for
completeness.

### 3. GLSL → LPIR Mapping Table

A comprehensive table mapping every Naga construct to LPIR. Organize by
Naga expression/statement variant.

#### Expressions

Cover every variant of `naga::Expression` that the lowering handles:

| Naga Expression | LPIR | Notes |
|---|---|---|
| `Literal(F32(v))` | `fconst.f32 v` | |
| `Literal(I32(v))` | `iconst.i32 v` | |
| `Literal(U32(v))` | `iconst.i32 v` | Reinterpret bits |
| `Literal(Bool(v))` | `iconst.i32 1` / `iconst.i32 0` | |
| `Constant(h)` | `fconst.f32` / `iconst.i32` | Resolve from module constants |
| `FunctionArgument(i)` | Parameter VReg `vi` | Direct mapping |
| `LocalVariable(h)` | VReg | One VReg per local var (scalar) |
| `Load { pointer }` | VReg read or `load` | LocalVar → VReg; other → `load` |
| `Binary { op, left, right }` | Core op | See binary mapping below |
| `Unary { op, expr }` | Core op | See unary mapping below |
| `Select { cond, accept, reject }` | `select` | |
| `As { expr, kind, convert }` | Cast op | `ftoi_sat_s`, `itof_s`, etc. |
| `ZeroValue(ty)` | `fconst.f32 0.0` / `iconst.i32 0` | Bool zero is `iconst.i32 0` |
| `Math { fun, args }` | `mathcall` | See MathFunc mapping |
| `CallResult(h)` | Result VReg from `call` | |
| `Compose { .. }` | Multiple VRegs | Scalarized — one VReg per component |
| `Splat { .. }` | `copy` to N VRegs | Scalarized |
| `Swizzle { .. }` | VReg selection | Scalarized — pick component VRegs |
| `AccessIndex { base, index }` | VReg selection | Scalarized — fixed index into component VRegs |

#### Binary op mapping

| Naga BinaryOperator | Float | Signed Int | Unsigned Int | Bool |
|---|---|---|---|---|
| Add | `fadd` | `iadd` | `iadd` | — |
| Subtract | `fsub` | `isub` | `isub` | — |
| Multiply | `fmul` | `imul` | `imul` | — |
| Divide | `fdiv` | `idiv_s` | `idiv_u` | — |
| Modulo | `mathcall fmod` | `irem_s` | `irem_u` | — |
| Equal | `feq` | `ieq` | `ieq` | `ieq` |
| NotEqual | `fne` | `ine` | `ine` | `ine` |
| Less | `flt` | `ilt_s` | `ilt_u` | — |
| LessEqual | `fle` | `ile_s` | `ile_u` | — |
| Greater | `fgt` | `igt_s` | `igt_u` | — |
| GreaterEqual | `fge` | `ige_s` | `ige_u` | — |
| LogicalAnd | — | — | — | `iand` |
| LogicalOr | — | — | — | `ior` |
| And | — | `iand` | `iand` | — |
| InclusiveOr | — | `ior` | `ior` | — |
| ExclusiveOr | — | `ixor` | `ixor` | — |
| ShiftLeft | — | `ishl` | `ishl` | — |
| ShiftRight | — | `ishr_s` | `ishr_u` | — |

#### Unary op mapping

| Naga UnaryOperator | Float | Signed Int | Bool |
|---|---|---|---|
| Negate | `fneg` | `ineg` | — |
| LogicalNot | — | — | `ieq` with `iconst.i32 0` |
| BitwiseNot | — | `ibnot` | — |

#### Statement mapping

| Naga Statement | LPIR |
|---|---|
| `Emit { range }` | No-op (expressions emitted on demand) |
| `Block(body)` | Emit body statements sequentially |
| `If { condition, accept, reject }` | `if v { ... } else { ... }` |
| `Loop { body, continuing, break_if }` | `loop { ... br_if_not ... continue }` |
| `Break` | `break` |
| `Continue` | `continue` |
| `Return { value }` | `return v` / `return` |
| `Store { pointer, value }` | VReg reassignment or `store` |
| `Call { function, arguments, result }` | `call @name(args)` |

#### Vector scalarization mapping

Document how vector operations are decomposed:

| GLSL / Naga | LPIR (scalarized) |
|---|---|
| `vec3 a + vec3 b` | 3× `fadd` on component VRegs |
| `vec3(1.0, 2.0, 3.0)` | 3× `fconst` + 3 VRegs |
| `a.xy` (swizzle) | Select VRegs for x, y components |
| `dot(a, b)` (vec3) | 3× `fmul` + 2× `fadd` |
| `cross(a, b)` | 6× `fmul` + 3× `fsub` |
| `length(a)` (vec3) | 3× `fmul` + 2× `fadd` + `mathcall fsqrt` |

## Validate

Review the section for:
- Every MathFunc has name, signature, GLSL equivalent, and description.
- The GLSL → LPIR mapping covers all currently-handled Naga variants.
- Binary and unary op mappings are complete for all type combinations.
- Vector scalarization examples are included.
- Cross-reference with the current WASM emitter to ensure no handled
  expression or statement type is missing from the mapping.

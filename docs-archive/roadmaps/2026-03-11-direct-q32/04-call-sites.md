# Call Site Inventory

Every place in the codegen that emits float-specific CLIF instructions needs
to route through the numeric strategy instead. This is the full inventory,
based on the current codebase.

## Inline float operations

### expr/literal.rs
- `f32const(*f)` — 1 call site
  → `ctx.numeric.emit_const(*f, ...)`

### expr/binary.rs (emit_scalar_binary_op)
- `fadd(a, b)` — 1 site (Add for Float)
- `fsub(a, b)` — 1 site (Sub for Float)
- `fmul(a, b)` — 1 site (Mult for Float)
- `fdiv(a, b)` — 1 site (Div for Float)
- `fcmp(cc, a, b)` — 6 sites (Equal, NonEqual, LT, GT, LTE, GTE for Float)
  → All route through `ctx.numeric.emit_*(...)`

### expr/unary.rs
- `fneg(val)` — 1 site
- `fabs(val)` — used via builtins
  → `ctx.numeric.emit_neg(...)`, `ctx.numeric.emit_abs(...)`

### expr/coercion.rs
- `fcvt_from_sint(types::F32, val)` — int-to-float conversion
- `fcvt_to_sint(types::I32, val)` — float-to-int conversion
  → `ctx.numeric.emit_from_int(...)`, `ctx.numeric.emit_to_int(...)`

### expr/matrix.rs
- `fadd`, `fsub` — per-component add/sub (~2 sites)
- `fmul` — scalar×matrix, matrix×matrix dot products (~3 sites)
  → Same as binary, through strategy

### builtins/common.rs
- `sqrt(val)` — 1 inline site
- `floor(val)` — 1 inline site + used in fract
- `ceil(val)` — 1 inline site
- `fmin(a, b)` — 1 site (min)
- `fmax(a, b)` — 1 site (max)
- `fabs(val)` — 1 site (abs)
- `f32const(0.0)`, `f32const(1.0)`, `f32const(-1.0)` — sign() implementation
  → Strategy methods for each

### builtins/geometric.rs
- `fmul`, `fadd` — dot product computation
- `fmul`, `fsub` — cross product
- `fdiv` — normalize
  → Already uses scalar ops; routes through strategy

### builtins/trigonometric.rs
- `f32const` — degree/radian conversion constants
  → `ctx.numeric.emit_const(...)`

## Type references

### codegen/signature.rs (SignatureBuilder)
- `Type::to_cranelift_type()` returns `types::F32` for GLSL floats
- Used for function signatures, parameter types, return types
  → Route through `strategy.scalar_type()` or `strategy.map_signature()`

### Various codegen files
- `types::F32` used for comparisons (checking if a type is float)
- These become `strategy.scalar_type()` comparisons

## Library calls (see 03-builtin-dispatch.md)

### builtins/helpers.rs
- `get_math_libcall(name)` — creates TestCase f32→f32
- `get_math_libcall_2arg(name)` — creates TestCase (f32,f32)→f32
  → Numeric-aware: float uses TestCase, Q32 uses BuiltinId lookup

### builtins/trigonometric.rs
- sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, asinh, acosh,
  atanh, radians, degrees
  → All go through `get_math_libcall` / `get_math_libcall_2arg`

### builtins/common.rs
- pow, exp, exp2, log, log2, inversesqrt, mod, round, roundEven
  → Same libcall path

### lpfn_fns.rs
- All LPFX function calls (noise, hash, color conversion, etc.)
  → Float vs Q32 variant selection

## Summary

~25 inline operation call sites + ~20 libcall sites + ~5 type reference
sites. Total: roughly 50 code locations to update.

Most are mechanical: replace `builder.ins().fadd(a, b)` with
`ctx.numeric.emit_add(a, b, builder)`. The libcall sites require a lookup
change. The type reference sites need `strategy.scalar_type()`.

This is not a small change, but each individual change is simple and
testable. A find-and-replace pass with manual verification would cover most
of it.

# Builtin Function Dispatch

The NumericStrategy trait handles inline operations (arithmetic, comparisons,
constants). Library/builtin calls — sin, cos, pow, noise functions, etc. —
need a separate mechanism.

## Current state

The codegen layer has three function call paths:

### 1. Math libcalls (builtins/helpers.rs)

```rust
fn get_math_libcall(name: &str, builder: &mut FunctionBuilder) -> FuncRef {
    // Creates ExternalName::testcase(name) with f32→f32 signature
}
fn get_math_libcall_2arg(name: &str, ...) -> FuncRef {
    // Same but (f32, f32) → f32
}
```

Used for: sinf, cosf, tanf, powf, expf, logf, sqrtf, fmodf, atan2f, etc.

The Q32 transform rewrites these calls: when it sees a `call` to a TestCase
function named "sinf", it replaces it with a call to the Q32 builtin
`LpQ32Sin`.

### 2. LPFX functions (lpfx_fns.rs)

LPFX (library pixel effects) functions have both float and Q32
implementations. The registry entry has a `q32_impl` field. Currently, the
codegen always calls the float variant, and the Q32 transform rewrites to
the Q32 variant.

### 3. User-defined functions (expr/function.rs)

These go through `declare_func_in_func` + `call`. The Q32 transform remaps
the FuncId to the Q32 module. With direct emission, user functions would be
declared with Q32 signatures from the start, so no remapping is needed.

## Proposed approach

### Math libcalls

Replace the hardcoded TestCase function names with a lookup through the
numeric mode:

```rust
// Before:
let func_ref = get_math_libcall("sinf", builder);
builder.ins().call(func_ref, &[val])

// After:
let func_ref = ctx.get_math_fn("sin", builder);
builder.ins().call(func_ref, &[val])
```

Where `get_math_fn` dispatches based on numeric mode:

- **Float:** Creates a TestCase external name "sinf" with f32 signature
  (same as today).
- **Q32:** Looks up the Q32 builtin (BuiltinId::LpQ32Sin), declares it in
  the module, returns its FuncRef with i32 signature.

This is a small change to `builtins/helpers.rs` and the call sites in
`builtins/common.rs` and `builtins/trigonometric.rs`.

The existing name mapping in `backend/transform/q32/calls.rs`
(`map_testcase_to_builtin`) provides the float-name → Q32-builtin mapping.
This can be extracted and reused.

### LPFX functions

The LPFX dispatch already knows both variants. Change the selection from
"always float, transform later" to "select based on DecimalFormat":

```rust
// Before (in lpfx_fns.rs):
let func_ref = get_lpfx_testcase_call(name, ...);  // always float

// After:
let func_ref = match ctx.decimal_format {
    DecimalFormat::Float => get_lpfx_testcase_call(name, ...),
    DecimalFormat::Q32 => get_lpfx_q32_call(name, ...),
};
```

The Q32 variant uses `find_lpfx_fn` → `lpfx_fn.q32_impl` → declare as
builtin → FuncRef. This logic already exists in the Q32 transform's call
conversion; it needs to move to the LPFX dispatch.

### User-defined functions

No change needed beyond using Q32 signatures when declaring functions.
The `SignatureBuilder` already builds float signatures; the strategy's
`map_signature` transforms them to Q32 before declaration. Cross-function
calls naturally use the correct (Q32) signatures.

### Inline expansions

Some "builtins" are expanded inline by the Q32 transform rather than
called as functions:

- `fract(x)` → `x - floor(x)` (in Q32 arithmetic)
- `sign(x)` → comparison + select
- `isinf(x)` → overflow sentinel comparison
- `isnan(x)` → constant false

These inline expansions would move to the corresponding builtin emission
functions in `builtins/common.rs`, guarded by numeric mode. Each already
has a codegen function that emits the float version; adding a Q32 branch
is straightforward.

## Scope

The changes are localized:

- `builtins/helpers.rs` — add numeric-aware math function lookup
- `builtins/common.rs` — Q32 branches for fract, sign, isinf, isnan;
  numeric-aware libcall selection for pow, exp, log, etc.
- `builtins/trigonometric.rs` — numeric-aware libcall selection for
  sin, cos, tan, etc.
- `lpfx_fns.rs` — select float vs Q32 variant based on format
- `expr/function.rs` — use mapped signatures for user function calls

The bulk of the math logic already exists in the Q32 transform. It's
being relocated, not rewritten.

# Phase 2: Implement Sin and Cos

## Goal

Port libfixmath's Taylor series implementation and wire it up through the transform.

## Tasks

### 2.1 Port libfixmath Sin Implementation

In `lp-glsl-builtins/src/q32/sin.rs`:

- Port libfixmath's accurate Taylor series sin implementation
- Use constants from libfixmath: `fix16_pi = 205887`
- Use `__lp_q32_mul` for multiplies
- Use Rust native division for constants (6, 120, 5040, 362880, 39916800)
- Handle range reduction: `inAngle % (fix16_pi << 1)` using Rust's `%` operator
- Reduce to [-π, π] range
- Implement Taylor series: `x - x³/6 + x⁵/120 - x⁷/5040 + x⁹/362880 - x¹¹/39916800`
- Export as `#[no_mangle] pub extern "C" fn __lp_q32_sin(x: i32) -> i32`

### 2.2 Implement Cos

In `lp-glsl-builtins/src/q32/cos.rs`:

- Implement as `sin(x + π/2)` (like libfixmath does)
- Export as `#[no_mangle] pub extern "C" fn __lp_q32_cos(x: i32) -> i32`

### 2.3 Add to Module

In `lp-glsl-builtins/src/q32/mod.rs`:

- Add `mod sin;` and `mod cos;`
- Export `__lp_q32_sin` and `__lp_q32_cos`

### 2.4 Update Builtins App

In `lp-glsl-builtins-emu-app/src/main.rs`:

- Add references to `__lp_q32_sin` and `__lp_q32_cos` in `main()` to prevent dead code elimination

### 2.5 Add Transform Conversion Logic

In `lp-glsl-compiler/src/backend/transform/q32/converters/calls.rs`:

- Modify `convert_call()` to detect TestCase calls to math functions
- Check if TestCase name matches mapping table
- If match found:
    - Get corresponding `BuiltinId` from mapping table
    - Get FuncId from `func_id_map` using builtin name
    - Create new call to `__lp_q32_*` function (similar to how `convert_sqrt` works)
    - Replace the TestCase call with the builtin call

## Success Criteria

- `__lp_q32_sin` and `__lp_q32_cos` compile and are exported
- Functions are referenced in builtins app
- Transform successfully converts TestCase calls to builtin calls
- All code compiles without warnings


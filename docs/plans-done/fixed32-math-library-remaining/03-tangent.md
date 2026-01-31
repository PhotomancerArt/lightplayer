# Phase 3: Tangent Function

## Goal

Implement `tan` function using libfixmath's approach (tan = sin/cos).

## Tasks

### 3.1 Port libfixmath Tan Implementation

In `lp-glsl-builtins/src/q32/tan.rs`:

- Port libfixmath's tan implementation: `tan(x) = sin(x) / cos(x)`
- Use `__lp_q32_sin` and `__lp_q32_cos`
- Use `__lp_q32_div` for division
- Handle edge cases (cos(x) = 0, etc.)
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_tan(x: i32) -> i32`

### 3.2 Add to Module

In `lp-glsl-builtins/src/q32/mod.rs`:

- Add `mod tan;`
- Export `__lp_q32_tan`

### 3.3 Update Builtins App

In `lp-glsl-builtins-emu-app/src/main.rs`:

- Add reference to `__lp_q32_tan` in `main()` to prevent dead code elimination

### 3.4 Add to Registry

In `lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Add `Q32Tan` to `BuiltinId` enum
- Add to `name()`, `signature()`, `all()`, and `get_function_pointer()`

### 3.5 Add Transform Conversion

In `lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- Add `"tanf"` and `"__lp_tan"` to `map_testcase_to_builtin()` mapping to `BuiltinId::Q32Tan`

### 3.6 Add Tests

In `lp-glsl-builtins/src/q32/tan.rs`:

- Add tests using `test_q32_function_relative()` helper
- Source test cases from libfixmath test suite
- Use 0.01 tolerance initially

## Success Criteria

- `__lp_q32_tan` compiles and is exported
- Function is referenced in builtins app
- Transform successfully converts TestCase/User calls to builtin calls
- Tests pass with 0.01 tolerance
- `builtins/phases/02-basic-trig.glsl` passes (tan test)
- All code compiles without warnings


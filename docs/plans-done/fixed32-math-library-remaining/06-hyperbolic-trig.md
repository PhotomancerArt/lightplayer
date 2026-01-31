# Phase 6: Hyperbolic Trig Functions

## Goal

Implement hyperbolic trigonometric functions using mathematical formulas with exp, log, and sqrt.

## Tasks

### 6.1 Implement Sinh

In `lp-glsl-builtins/src/q32/sinh.rs`:

- Implement using: `sinh(x) = (exp(x) - exp(-x)) / 2`
- Use `__lp_q32_exp` and `__lp_q32_div`
- Handle overflow cases
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_sinh(x: i32) -> i32`

### 6.2 Implement Cosh

In `lp-glsl-builtins/src/q32/cosh.rs`:

- Implement using: `cosh(x) = (exp(x) + exp(-x)) / 2`
- Use `__lp_q32_exp` and `__lp_q32_div`
- Handle overflow cases
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_cosh(x: i32) -> i32`

### 6.3 Implement Tanh

In `lp-glsl-builtins/src/q32/tanh.rs`:

- Implement using: `tanh(x) = sinh(x) / cosh(x)`
- Use `__lp_q32_sinh` and `__lp_q32_cosh`
- Handle edge cases (cosh(x) = 0, etc.)
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_tanh(x: i32) -> i32`

### 6.4 Implement Asinh

In `lp-glsl-builtins/src/q32/asinh.rs`:

- Implement using: `asinh(x) = log(x + sqrt(x² + 1))`
- Use `__lp_q32_log`, `__lp_q32_sqrt`, `__lp_q32_mul`, `__lp_q32_add`
- Handle domain restrictions
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_asinh(x: i32) -> i32`

### 6.5 Implement Acosh

In `lp-glsl-builtins/src/q32/acosh.rs`:

- Implement using: `acosh(x) = log(x + sqrt(x² - 1))` for x >= 1
- Use `__lp_q32_log`, `__lp_q32_sqrt`, `__lp_q32_mul`, `__lp_q32_sub`
- Handle domain restrictions (x >= 1)
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_acosh(x: i32) -> i32`

### 6.6 Implement Atanh

In `lp-glsl-builtins/src/q32/atanh.rs`:

- Implement using: `atanh(x) = (1/2) * log((1+x)/(1-x))` for |x| < 1
- Use `__lp_q32_log`, `__lp_q32_div`, `__lp_q32_add`, `__lp_q32_sub`
- Handle domain restrictions (|x| < 1)
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_atanh(x: i32) -> i32`

### 6.7 Add to Module

In `lp-glsl-builtins/src/q32/mod.rs`:

- Add all hyperbolic modules
- Export all functions

### 6.8 Update Builtins App

In `lp-glsl-builtins-emu-app/src/main.rs`:

- Add references to all hyperbolic functions

### 6.9 Add to Registry

In `lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Add all hyperbolic BuiltinIds to enum
- All are (i32) -> i32 signatures
- Add to all registry functions

### 6.10 Add Transform Conversion

In `lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- Add mappings for all hyperbolic functions (sinhf, coshf, tanhf, asinhf, acoshf, atanhf and __lp_*
  versions)
- All map to 1-arg functions

### 6.11 Add Tests

- Add tests for each function using `test_q32_function_relative()` helper
- Source test cases from mathematical reference or create based on known values
- Use 0.01 tolerance initially, may need adjustment

## Success Criteria

- All hyperbolic functions compile and are exported
- Functions are referenced in builtins app
- Transform successfully converts function calls
- Tests pass with appropriate tolerance
- `builtins/phases/04-hyperbolic-trig.glsl` passes
- All code compiles without warnings


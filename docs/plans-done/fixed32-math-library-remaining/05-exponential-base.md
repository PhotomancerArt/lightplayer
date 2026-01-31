# Phase 5: Exponential Base Functions

## Goal

Implement exponential and logarithmic base functions (exp, log, exp2, log2) needed by hyperbolic and
power functions.

## Tasks

### 5.1 Port libfixmath Exp Implementation

In `lp-glsl-builtins/src/q32/exp.rs`:

- Port libfixmath's exp implementation (power series)
- Uses: exp(x) = 1 + x + x²/2! + x³/3! + ...
- Handle negative x: exp(-x) = 1/exp(x)
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_exp(x: i32) -> i32`

### 5.2 Port libfixmath Log Implementation

In `lp-glsl-builtins/src/q32/log.rs`:

- Port libfixmath's log implementation (Newton-Raphson method)
- Uses iterative refinement with scaling
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_log(x: i32) -> i32`

### 5.3 Port fr_math Exp2 Implementation

In `lp-glsl-builtins/src/q32/exp2.rs`:

- Port fr_math's pow2 (exp2) implementation
- Uses bit manipulation and polynomial approximation
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_exp2(x: i32) -> i32`

### 5.4 Port libfixmath Log2 Implementation

In `lp-glsl-builtins/src/q32/log2.rs`:

- Port libfixmath's log2 implementation
- Uses log2(x) = log(x) / log(2) relationship or direct implementation
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_log2(x: i32) -> i32`

### 5.5 Add to Module

In `lp-glsl-builtins/src/q32/mod.rs`:

- Add `mod exp;`, `mod log;`, `mod exp2;`, `mod log2;`
- Export all functions

### 5.6 Update Builtins App

In `lp-glsl-builtins-emu-app/src/main.rs`:

- Add references to all exponential functions

### 5.7 Add to Registry

In `lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Add `Q32Exp`, `Q32Log`, `Q32Exp2`, `Q32Log2` to `BuiltinId` enum
- All are (i32) -> i32 signatures
- Add to all registry functions

### 5.8 Add Transform Conversion

In `lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- Add mappings: `"expf"`, `"logf"`, `"exp2f"`, `"log2f"` and `"__lp_exp"`, `"__lp_log"`,
  `"__lp_exp2"`, `"__lp_log2"`
- All map to 1-arg functions

### 5.9 Add Tests

- Add tests for each function using `test_q32_function_relative()` helper
- Source test cases from libfixmath and fr_math test suites
- Use 0.01 tolerance initially

## Success Criteria

- All exponential functions compile and are exported
- Functions are referenced in builtins app
- Transform successfully converts function calls
- Tests pass with 0.01 tolerance
- `builtins/phases/05-exponential.glsl` passes (exp, log, exp2, log2 tests)
- All code compiles without warnings


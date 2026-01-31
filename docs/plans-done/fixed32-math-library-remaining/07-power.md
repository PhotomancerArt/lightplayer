# Phase 7: Power Function

## Goal

Implement `pow(x, y)` function using fpm's approach (pow(x,y) = exp2(log2(x) * y)).

## Tasks

### 7.1 Port fpm Pow Implementation

In `lp-glsl-builtins/src/q32/pow.rs`:

- Port fpm's pow implementation
- Uses: `pow(x, y) = exp2(log2(x) * y)` for fractional exponents
- Handle special cases: pow(x, 0) = 1, pow(0, y) = 0 for y > 0, etc.
- Handle integer exponents separately for efficiency
- Export as `#[unsafe(no_mangle)] pub extern "C" fn __lp_q32_pow(x: i32, y: i32) -> i32`
- Signature: (i32, i32) -> i32

### 7.2 Add to Module

In `lp-glsl-builtins/src/q32/mod.rs`:

- Add `mod pow;`
- Export `__lp_q32_pow`

### 7.3 Update Builtins App

In `lp-glsl-builtins-emu-app/src/main.rs`:

- Add reference to `__lp_q32_pow`

### 7.4 Add to Registry

In `lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Add `Q32Pow` to `BuiltinId` enum
- Signature: (i32, i32) -> i32
- Add to all registry functions

### 7.5 Add Transform Conversion

In `lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- Add mappings: `"powf"` and `"__lp_pow"` -> `(Q32Pow, 2)`
- This should already work with the 2-arg support added in Phase 4

### 7.6 Add Tests

- Add tests using `test_q32_function_relative()` helper
- Source test cases from fpm test suite
- Use 0.01 tolerance initially

## Success Criteria

- `__lp_q32_pow` compiles and is exported
- Function is referenced in builtins app
- Transform successfully converts 2-arg pow calls
- Tests pass with 0.01 tolerance
- `builtins/phases/05-exponential.glsl` passes (pow tests)
- All code compiles without warnings


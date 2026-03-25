# Phase 7: Tests

## Scope

Write integration tests that validate the full GLSL → LPIR pipeline.
Two test files:
- `lower_interp.rs`: GLSL → LPIR → interpret → verify computed results
- `lower_print.rs`: GLSL → LPIR → print → verify text output

## Code Organization Reminders

- Tests should be concise and readable, ideally under 20 lines.
- Use helper functions liberally — if test logic is repeated, extract it.
- Test helpers should be at the bottom of test files.
- Prefer clear test names over inline comments.
- Avoid debug `println!` in tests unless debugging a specific failure.
- Each test should test one thing clearly.
- NEVER change a test to make it pass when it fails due to a bug.

## Implementation Details

### Test helpers (bottom of each file)

```rust
fn compile_and_lower(glsl: &str) -> lpir::module::IrModule {
    let naga = lp_glsl_naga::compile(glsl).unwrap();
    lp_glsl_naga::lower::lower(&naga).unwrap()
}

fn run(glsl: &str, func: &str, args: &[Value]) -> Vec<Value> {
    let module = compile_and_lower(glsl);
    let mut handler = StdMathHandler; // or a combined handler
    lpir::interp::interpret(&module, func, args, &mut handler).unwrap()
}

fn run_f32(glsl: &str, func: &str, args: &[Value]) -> f32 {
    run(glsl, func, args)[0].as_f32().unwrap()
}

fn run_i32(glsl: &str, func: &str, args: &[Value]) -> i32 {
    run(glsl, func, args)[0].as_i32().unwrap()
}

fn assert_f32_close(actual: f32, expected: f32, epsilon: f32) {
    assert!((actual - expected).abs() < epsilon,
        "expected {expected}, got {actual}");
}
```

### `lower_interp.rs` — interpreter tests

#### Arithmetic basics
- `interp_float_add`: `float f(float a, float b) { return a + b; }`
- `interp_float_sub_mul_div`: test -, *, /
- `interp_int_arithmetic`: `int f(int a, int b) { return a + b; }` etc.
- `interp_float_negation`: `float f(float x) { return -x; }`
- `interp_int_negation`: `int f(int x) { return -x; }`

#### Constants and literals
- `interp_literal_return`: `float f() { return 42.0; }`
- `interp_int_literal`: `int f() { return 7; }`
- `interp_bool_literal`: `bool f() { return true; }` → i32(1)

#### Comparisons
- `interp_float_comparisons`: test <, <=, >, >=, ==, !=
- `interp_int_comparisons`: same for int

#### Control flow
- `interp_if_else`: `float f(float x) { if (x > 0.0) return 1.0; else return -1.0; }`
- `interp_loop_sum`: `int f(int n) { int s = 0; for (int i = 0; i < n; i++) s += i; return s; }`
- `interp_loop_break`: loop with explicit break condition
- `interp_while_loop`: while-style loop via Naga's Loop + break_if
- `interp_nested_if`: nested if/else chains

#### Local variables
- `interp_local_var`: `float f(float x) { float y = x * 2.0; return y + 1.0; }`
- `interp_multiple_locals`: multiple local variables with reassignment

#### Casts
- `interp_float_to_int`: `int f(float x) { return int(x); }`
- `interp_int_to_float`: `float f(int x) { return float(x); }`

#### Select
- `interp_ternary`: `float f(float x) { return x > 0.0 ? 1.0 : -1.0; }`

#### User function calls
- `interp_call_user_func`: `float double(float x) { return x * 2.0; } float f(float x) { return double(x) + 1.0; }`
- `interp_call_chain`: A calls B calls C, verify result

#### Math builtins — Tier 1
- `interp_abs_float`: `float f(float x) { return abs(x); }`
- `interp_abs_int`: `int f(int x) { return abs(x); }`
- `interp_sqrt`: `float f(float x) { return sqrt(x); }`
- `interp_floor_ceil_trunc`: test all three
- `interp_min_max_float`: test min/max
- `interp_min_max_int`: test min/max for integers

#### Math builtins — Tier 2
- `interp_mix`: `float f(float a, float b, float t) { return mix(a, b, t); }`
- `interp_smoothstep`: verify polynomial shape (0 at edge0, 1 at edge1)
- `interp_step`: test edge cases
- `interp_clamp`: test clamp within and outside range
- `interp_sign`: test positive, negative, zero
- `interp_fract`: test fractional part
- `interp_fma`: `float f(float a, float b, float c) { return fma(a, b, c); }`

#### Math builtins — Tier 3
- `interp_sin_cos`: verify sin(0)≈0, cos(0)≈1, sin(π/2)≈1
- `interp_pow`: `float f(float x, float y) { return pow(x, y); }`
- `interp_exp_log`: verify exp(0)=1, log(1)=0

### `lower_print.rs` — text output tests

These verify the LPIR text form looks correct (stable format for
snapshot-style tests). Use `lpir::print::print_module()`.

- `print_simple_add`: verify func signature, fadd op, return
- `print_if_else`: verify if/else block structure
- `print_loop`: verify loop { ... } structure
- `print_math_import`: verify `import @std.math::sin(f32) -> f32` line
- `print_call`: verify `call @func_name(...)` syntax

These can be smaller and more focused — just check that key patterns
appear in the printed output rather than exact string matching (to avoid
brittleness with VReg numbering).

## Validate

```
cargo test -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
```

All tests pass. The full pipeline is exercised end-to-end.

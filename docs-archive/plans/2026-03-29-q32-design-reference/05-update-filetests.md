# Phase 5: Update filetests

## Scope

Bring filetests into agreement with `docs/design/q32.md` for Q32 edge cases:
`isnan`, `isinf`, division by zero.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment

## Implementation Details

### 5a. `common-isinf.glsl`

Current state: expects IEEE behavior (`isinf(1.0/0.0) == true`), first test
has `@unimplemented(backend=jit)`. No Q32-specific handling.

Per design doc §6 + §7: `isinf` always returns `false` on Q32. Tests that
expect `true` from `isinf` need `@unsupported(float_mode=q32, reason="...")`.

Update:

- Add file-level
  `@unsupported(float_mode=q32, reason="Q32 has no Inf encoding; isinf is always false")`
  This skips the entire file under Q32, which is correct — every test that
  expects `true` from `isinf` is IEEE-only.
- Remove per-test `@unimplemented(backend=jit)` on `test_isinf_normal` and
  `test_isinf_zero` (these return `false`, which Q32 also returns, but the
  file is skipped entirely under Q32 anyway due to the tests that expect
  `true` — cleaner to skip the whole file).

### 5b. `common-isnan.glsl`

Current state: expects IEEE behavior for some tests, first test has
`@unimplemented(backend=jit)`. All expected results are `false` (no way to
create NaN via GLSL literals that Naga accepts).

Since all expectations are `false` (which matches Q32), the file could
theoretically run under Q32. However, the tests that use `1.0/0.0` as an
intermediate value have different behavior on Q32 (saturates instead of Inf).
The `isnan(inf)` tests assume IEEE inf exists.

Update:

- Add file-level
  `@unsupported(float_mode=q32, reason="Q32 has no NaN encoding; tests assume IEEE intermediates")`
- Remove per-test `@unimplemented(backend=jit)` annotations (the file is
  now properly scoped).

### 5c. `scalar/float/op-divide.glsl` — add Q32 div-by-zero tests

Current state: only normal division cases. No edge cases.

Add new test functions for div-by-zero behavior:

```glsl
float test_float_divide_pos_by_zero() {
    float a = 1.0;
    float b = 0.0;
    return a / b;
}

// @unsupported(float_mode=q32, reason="Q32 div-by-zero saturates to max, not IEEE Inf")
// run: test_float_divide_pos_by_zero() ~= inf
```

Optionally, add a **separate** Q32-specific div-by-zero filetest (e.g.
`scalar/float/q32-div-by-zero.glsl`) that tests the Q32-specific behavior
under `jit.q32` target:

```glsl
// test run

float test_q32_div_pos_by_zero() {
    float a = 1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_pos_by_zero() ~= 32767.99998

float test_q32_div_neg_by_zero() {
    float a = -1.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_neg_by_zero() ~= -32768.0

float test_q32_div_zero_by_zero() {
    float a = 0.0;
    float b = 0.0;
    return a / b;
}

// run: test_q32_div_zero_by_zero() ~= 0.0
```

Whether to use `~=` with the max Q32 float value or exact integer comparison
depends on filetest infrastructure — check what `~=` tolerance is and whether
it can match near-max values.

### 5d. Review `edge-nan-inf-propagation.glsl`

Already has file-level `@unsupported(float_mode=q32, ...)`. Confirm it
matches the design doc's language. No changes expected.

## Validate

```bash
cargo test -p lps-filetests
```

Or if there's a more targeted command for running specific filetests,
use that. The key is that no filetest should fail under Q32 mode due to
the changes in this plan.

# Phase 2: Verify Div Builtin Edge Cases

## Goal

Verify that `__lp_q32_div` handles all edge cases correctly, especially small divisors (< 2^16) that
the inline code currently handles.

## Tasks

### 2.1 Review div.rs Implementation

Examine `lp-glsl/lp-glsl-builtins/src/builtins/q32/div.rs`:

- Verify division-by-zero handling (saturates to max/min based on sign)
- Check if small divisors (< 2^16) are handled correctly
- Compare with inline code logic in `convert_fdiv`

### 2.2 Test Edge Cases

Create or update tests to verify:

- Division by zero (positive and negative numerators)
- Small divisors (< 2^16)
- Large divisors
- Normal cases
- Edge values (MIN_FIXED, MAX_FIXED)

### 2.3 Fix if Needed

If the builtin doesn't handle edge cases correctly:

- Update `__lp_q32_div` to match inline code behavior
- Ensure it handles small divisors correctly
- Ensure division-by-zero saturation matches inline code

## Success Criteria

- `__lp_q32_div` verified to handle all edge cases correctly
- Tests pass for all edge cases
- If fixes were needed, they are implemented and tested
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented

# Phase 1: Implement Add and Sub Builtins

## Goal

Create `__lp_q32_add` and `__lp_q32_sub` builtin functions following the pattern established by
`__lp_q32_mul`.

## Tasks

### 1.1 Create add.rs

Create `lp-glsl/lp-glsl-builtins/src/builtins/q32/add.rs`:

- Implement `__lp_q32_add(a: i32, b: i32) -> i32`
- Use i64 for intermediate calculation to avoid overflow
- Clamp result to [MIN_FIXED, MAX_FIXED]
- Add tests similar to `mul.rs` tests
- Use same constants as `mul.rs`: `MAX_FIXED = 0x7FFF_FFFF`, `MIN_FIXED = i32::MIN`

### 1.2 Create sub.rs

Create `lp-glsl/lp-glsl-builtins/src/builtins/q32/sub.rs`:

- Implement `__lp_q32_sub(a: i32, b: i32) -> i32`
- Use i64 for intermediate calculation to avoid overflow
- Clamp result to [MIN_FIXED, MAX_FIXED]
- Add tests similar to `mul.rs` tests
- Use same constants as `mul.rs`

### 1.3 Add #[unsafe(no_mangle)] Attributes

Both functions must have:

- `#[unsafe(no_mangle)]` attribute
- `pub extern "C"` calling convention
- Function names starting with `__lp_q32_`

## Success Criteria

- `add.rs` and `sub.rs` files created
- Both builtins compile without errors
- Both builtins have comprehensive tests
- Tests pass
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

# Phase 1: Update find_lpfx_fn to support overload resolution

## Description

Update `find_lpfx_fn` in `lpfx_fn_registry.rs` to accept `arg_types` parameter and implement overload resolution logic. The function should find all functions with matching name, then match on parameter types using exact type matching.

## Implementation

### File: `lp-glsl/crates/lp-glsl-compiler/src/frontend/semantic/lpfx/lpfx_fn_registry.rs`

**Update `find_lpfx_fn` signature:**
- Change from: `find_lpfx_fn(name: &str) -> Option<&'static LpfxFn>`
- Change to: `find_lpfx_fn(name: &str, arg_types: &[Type]) -> Option<&'static LpfxFn>`

**Implement overload resolution:**
1. Find all functions with matching GLSL name
2. Filter to functions with matching parameter count
3. For each candidate, check exact type match for all parameters:
   - Scalar types: exact match required
   - Vector types: exact match required (including component count)
4. Return first exact match, or None if ambiguous/no match

**Add helper function for parameter type matching:**
- `matches_signature(func: &LpfxFn, arg_types: &[Type]) -> bool`
- Checks parameter count matches
- Checks each parameter type matches exactly

## Success Criteria

- `find_lpfx_fn` signature updated to require `arg_types`
- Overload resolution implemented with exact type matching
- Returns correct function for matching signature
- Returns None for ambiguous or no match cases
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`

## Style Notes

### Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

### Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

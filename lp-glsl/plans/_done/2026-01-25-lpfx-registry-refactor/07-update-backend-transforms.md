# Phase 7: Update Backend Transforms to Use Registry

## Description

Update backend transform code to use the registry for testcase name mapping.

## Implementation

### File: `backend/transform/q32/converters/math.rs`

Replace:
- Hardcoded `map_testcase_to_builtin` function with registry lookup
- Look up function by testcase name (e.g., "__lpfx_snoise1") in registry
- Use `impl.rust_fn_name` to match testcase names
- Return `BuiltinId` from registry lookup

## Success Criteria

- Backend transforms use registry
- Testcase name mapping works correctly
- No hardcoded function name checks
- Code compiles
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

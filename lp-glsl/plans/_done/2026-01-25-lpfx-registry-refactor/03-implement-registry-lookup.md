# Phase 3: Implement Registry Lookup Functions

## Description

Implement the registry API in `lpfx_fn_registry.rs` for looking up functions and validating calls.

## Implementation

### File: `frontend/semantic/lpfx/lpfx_fn_registry.rs`

Implement functions:
- `is_lpfx_fn(name: &str) -> bool` - Check if name starts with "lpfx_"
- `find_lpfx_fn(name: &str) -> Option<&LpfxFn>` - Lookup function by GLSL name from `LPFX_FNS`
- `check_lpfx_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String>` - Validate call matches signature and return return type
- `get_impl_for_format(fn: &LpfxFn, format: DecimalFormat) -> Option<&LpfxFnImpl>` - Find implementation for decimal format

## Success Criteria

- All lookup functions implemented
- `check_lpfx_fn_call` validates signatures correctly
- Handles vector types correctly (vec2, vec3)
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

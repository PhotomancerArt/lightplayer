# Phase 6: Update Codegen to Use Registry

## Description

Update codegen code to use the registry for all function information instead of `LpfxFnId` methods.

## Implementation

### File: `frontend/codegen/expr/function.rs`

Replace:
- `is_lp_lib_fn` check with `is_lpfx_fn` from registry
- Function call routing to use `find_lpfx_fn` and registry

### File: `frontend/codegen/lp_lib_fns.rs`

Replace:
- `LpfxFnId::from_name_and_args` with `find_lpfx_fn`
- `lp_fn.builtin_id()` with registry lookup
- `lp_fn.symbol_name()` with `impl.rust_fn_name`
- `lp_fn.needs_q32_mapping()` with checking `impl.decimal_format`
- `lp_fn.return_type()` with `fn.glsl_sig.return_type`
- Use signature helpers for argument expansion and type conversion
- Use `get_impl_for_format` to find correct implementation

## Success Criteria

- Codegen uses registry for all function info
- No `LpfxFnId` method calls
- Function calls generate correctly
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

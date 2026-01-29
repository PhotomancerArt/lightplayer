# Phase 9: Update lp-builtin-gen to Use Registry

## Description

Update the builtin generator tool to use the registry instead of `LpfxFnId::all()`.

## Implementation

### File: `apps/lp-builtin-gen/src/main.rs`

Replace:
- `LpfxFnId::all()` with iterating over `LPFX_FNS`
- `LpfxFnId` method calls with registry lookups
- Use `fn.glsl_sig.name` and `impl.rust_fn_name` for mappings

## Success Criteria

- Builtin generator uses registry
- No `LpfxFnId` usage
- Generator produces correct output
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

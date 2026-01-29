# Phase 1: Update Core Data Structures

## Description

Update the core data structures in `lpfx_fn.rs` to match the design. Ensure `LpfxFn` and `LpfxFnImpl` have all necessary fields for the registry system.

## Implementation

### File: `frontend/semantic/lpfx/lpfx_fn.rs`

Update structures:
- Ensure `LpfxFn` has `glsl_sig: FunctionSignature` and `impls: Vec<LpfxFnImpl>`
- Ensure `LpfxFnImpl` has `decimal_format: Option<DecimalFormat>`, `builtin_module: &'static str`, and `rust_fn_name: &'static str`
- Remove or update `LpfxImplType` enum if not needed

## Success Criteria

- Structures match the design
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

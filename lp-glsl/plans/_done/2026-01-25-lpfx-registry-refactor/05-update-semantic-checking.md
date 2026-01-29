# Phase 5: Update Semantic Checking to Use Registry

## Description

Update semantic checking code to use the registry instead of hardcoded checks.

## Implementation

### File: `frontend/semantic/type_check/inference.rs`

Replace:
- Hardcoded `LpfxFnId` checks with `check_lpfx_fn_call` from registry
- Remove any match statements on function names

### File: `frontend/semantic/lpfx/mod.rs`

Ensure registry module is properly exported.

## Success Criteria

- Semantic checking uses registry
- No hardcoded function name checks
- Type inference works correctly
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

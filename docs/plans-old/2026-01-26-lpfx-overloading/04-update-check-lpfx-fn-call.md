# Phase 4: Update check_lpfn_fn_call to use new find_lpfn_fn

## Description

Update `check_lpfn_fn_call` to use the new `find_lpfn_fn` signature that requires `arg_types`.
Extract the return type from the resolved function.

## Implementation

### File: `lp-shader/lps-compiler/src/frontend/semantic/lpfn/lpfn_fn_registry.rs`

**Update `check_lpfn_fn_call`:**

- Currently calls `find_lpfn_fn(name)` then validates types
- Change to: Call `find_lpfn_fn(name, arg_types)` which does resolution
- Extract return type from the resolved function
- Update error messages if needed (function already receives `arg_types`)

**Remove duplicate validation:**

- `find_lpfn_fn` now handles type matching, so `check_lpfn_fn_call` can be simplified
- Keep `check_lpfn_fn_call` as a convenience wrapper that returns the return type

## Success Criteria

- `check_lpfn_fn_call` uses new `find_lpfn_fn` signature
- Return type correctly extracted from resolved function
- Error messages updated appropriately
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
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

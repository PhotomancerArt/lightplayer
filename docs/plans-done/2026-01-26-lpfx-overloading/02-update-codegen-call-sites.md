# Phase 2: Update codegen call sites to pass arg_types

## Description

Update all call sites of `find_lpfx_fn` in codegen modules to extract argument types from the
available arguments and pass them to the updated function signature.

## Implementation

### File: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lpfx_fns.rs`

**Update `emit_lp_lib_fn_call`:**

- Extract `arg_types: Vec<Type>` from `args: Vec<(Vec<Value>, Type)>`
- Pass `arg_types` to `find_lpfx_fn(name, &arg_types)`
- Update error message if needed to handle ambiguous/no match cases

### File: `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lp_lib_fns.rs`

**Update `emit_lp_lib_fn_call`:**

- Extract `arg_types: Vec<Type>` from `args: Vec<(Vec<Value>, Type)>`
- Pass `arg_types` to `find_lpfx_fn(name, &arg_types)`
- Update error message if needed to handle ambiguous/no match cases

## Success Criteria

- Both codegen modules updated to pass `arg_types` to `find_lpfx_fn`
- Argument types correctly extracted from function arguments
- Error handling updated for ambiguous/no match cases
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

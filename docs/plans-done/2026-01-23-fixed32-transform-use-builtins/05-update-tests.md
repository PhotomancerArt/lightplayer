# Phase 5: Update Tests and Verify Correctness

## Goal

Unignore the div test and verify all tests pass, including filetests.

## Tasks

### 5.1 Unignore Div Test

In `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/arithmetic.rs`:

- Remove `#[ignore]` attribute from `test_q32_fdiv`
- Test should now pass with the builtin

### 5.2 Run Unit Tests

Execute tests for arithmetic converters:

- `cargo test --package lp-glsl-compiler --lib backend::transform::q32::converters::arithmetic`
- Verify all tests pass:
    - `test_q32_fadd`
    - `test_q32_fsub`
    - `test_q32_fmul`
    - `test_q32_fdiv` (now unignored)

### 5.3 Run Filetests

Execute filetests to verify correctness:

- `scripts/glsl-filetests.sh scalar/float`
- Verify all tests pass, especially:
    - `scalar/float/op-add.glsl`
    - `scalar/float/op-subtract.glsl`
    - `scalar/float/op-divide.glsl`
    - `scalar/float/op-multiply.glsl`

### 5.4 Verify Transform Works

Test that the transform correctly uses builtins:

- Check generated CLIF to verify function calls instead of inline code
- Verify builtin calls are present in post-transform CLIF

## Success Criteria

- `test_q32_fdiv` unignored and passing
- All unit tests pass
- All relevant filetests pass
- Transform correctly generates builtin calls
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

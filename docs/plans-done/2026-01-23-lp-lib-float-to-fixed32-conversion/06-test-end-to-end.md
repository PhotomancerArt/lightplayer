# Phase 6: Test End-to-End Flow

## Goal

Verify the complete flow from GLSL codegen → q32 transform → runtime works correctly for LP library functions.

## Tasks

### 6.1 Test Simplex Functions

Create or update tests that verify:
- GLSL code with `lpfx_snoise3()` call compiles
- Codegen emits TestCase call to `"__lpfx_snoise3"`
- Transform converts to call to `__lp_q32_lpfx_snoise3`
- Runtime executes correctly and returns expected values

### 6.2 Test Hash Functions

Verify hash functions still work:
- GLSL code with `lpfx_hash()` call compiles
- Codegen uses direct builtin call (no TestCase conversion)
- Runtime executes correctly

### 6.3 Test Multiple Functions

Test combinations:
- Multiple simplex calls in same shader
- Mix of simplex and hash calls
- Vector argument flattening works correctly

### 6.4 Verify No Regressions

Run existing tests:
- Ensure no existing functionality broke
- Verify other builtin functions still work
- Check that transform still works for `sin`/`cos` functions

## Success Criteria

- Simplex functions work end-to-end (codegen → transform → runtime)
- Hash functions continue to work correctly
- No regressions in existing functionality
- All tests pass
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented

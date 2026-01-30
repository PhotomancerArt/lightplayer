# Phase 5: Test and cleanup

## Description

Regenerate `lpfx_fns.rs`, run filetests to verify overload resolution works, fix any issues, and clean up any temporary code or warnings.

## Implementation

### Regenerate Builtins

1. Run `scripts/build-builtins.sh` to regenerate `lpfx_fns.rs`
2. Verify that `lpfx_hsv2rgb` has both vec3 and vec4 entries
3. Check that all overloaded functions have multiple entries

### Fix CI Issues

1. Run `just fix ci` to ensure all formatting and linting passes
2. Fix any issues that arise

### Run Tests

1. Run all lpfx filetests: `scripts/glsl-filetests.sh lpfx/lp_`
2. Verify both `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)` work correctly
3. Ensure all lpfx filetests pass

### Cleanup

1. Fix any warnings
2. Remove any temporary code or debug prints
3. Ensure all code is formatted with `cargo +nightly fmt`
4. Verify code compiles without warnings

## Success Criteria

- `lpfx_fns.rs` regenerated with multiple entries for overloaded functions
- `just fix ci` passes without errors
- All lpfx filetests pass (`scripts/glsl-filetests.sh lpfx/lp_`)
- Both `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)` work correctly
- No warnings in codebase
- Code formatted with `cargo +nightly fmt`
- All code compiles successfully

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

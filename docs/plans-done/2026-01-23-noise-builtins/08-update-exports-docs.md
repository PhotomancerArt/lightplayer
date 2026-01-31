# Phase 8: Update Exports and Documentation

## Description

Update `mod.rs` to export new functions and add documentation. Ensure all functions are properly
exported and documented.

## Implementation

### File: `lp-glsl-builtins/src/builtins/q32/mod.rs`

The builtin generator should have already updated this file, but verify:

- `mod lpfx_hash;` is included
- `mod lpfx_snoise1;` is included
- `mod lpfx_snoise2;` is included
- `mod lpfx_snoise3;` is included
- `pub use` statements for all functions are present

### Documentation

Add or update:

1. Module-level documentation explaining LP library functions
2. Function-level documentation for each function
3. Examples of usage in GLSL
4. Notes about Q32 fixed-point format

### Integration Documentation

Update compiler documentation if needed:

- How LP library functions differ from GLSL builtins
- How to add new LP library functions
- Vector argument handling

## Success Criteria

- All functions are exported from `mod.rs`
- Documentation is clear and accurate
- Examples demonstrate correct usage
- Code formatted with `cargo +nightly fmt`

## Notes

- Documentation should explain Q32 fixed-point format
- Include GLSL usage examples
- Note that functions are callable from GLSL with `lp_*` prefix
- Reference noise-rs and noiz for algorithm details

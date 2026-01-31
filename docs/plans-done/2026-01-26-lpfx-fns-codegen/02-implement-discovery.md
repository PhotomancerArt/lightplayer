# Phase 2: Implement discovery of LPFX functions with attributes

## Description

Discover all LPFX functions in the `lp-glsl-builtins/src/builtins/lpfx` directory that have
`#[lpfx_impl(...)]` attributes.

## Implementation

1. Create `lp-glsl-builtin-gen-app/src/discovery.rs` (or extend existing discovery)
2. Add function to walk directory tree and find Rust files
3. Parse Rust files using `syn::parse_file`
4. Find all functions with `#[lpfx_impl]` attribute
5. Extract function name and map to `BuiltinId` (reuse existing logic)
6. Return `LpfxFunctionInfo` structures

## Success Criteria

- Can discover all LPFX functions with attributes
- Correctly identifies functions with and without attributes
- Maps function names to correct `BuiltinId` values
- Code compiles

# Plan: LP Library Functions for Noise Generation

## Overview

Implement Lightplayer-specific library functions for noise generation that can be called from GLSL shaders. These functions provide a standard library for shader programming, similar to GLSL builtins but specific to Lightplayer's needs.

The implementation includes:
- Hash function (`lpfx_hash`) with 1D, 2D, and 3D variants
- Simplex noise functions (`lpfx_snoise1`, `lpfx_snoise2`, `lpfx_snoise3`)
- Integration with the existing builtin system
- Semantic checking and codegen for user-facing `lp_*` function names

## Phases

1. **Implement hash function** - Create `lpfx_hash.rs` with Q32 fixed-point hash functions using noiz algorithm
2. **Implement Simplex noise functions** - Create `lpfx_snoise1.rs`, `lpfx_snoise2.rs`, `lpfx_snoise3.rs` with Q32 implementations
3. **Regenerate builtin registry** - Run builtin generator to add new functions to `BuiltinId` enum
4. **Add semantic checking** - Create semantic checking to map `lp_*` names to `BuiltinId` variants
5. **Add codegen support** - Create codegen to generate calls to builtins with vector argument flattening
6. **Integrate into function call routing** - Update function call routing to check LP library functions
7. **Add tests** - Create tests comparing against noise-rs reference implementation
8. **Update exports and documentation** - Update `mod.rs` and add documentation

## Success Criteria

- All functions compile and are registered in the builtin system
- Functions can be called from GLSL with correct type checking
- Vector arguments are properly flattened and passed to internal functions
- Tests pass comparing against noise-rs reference implementation
- Code is formatted and follows project conventions

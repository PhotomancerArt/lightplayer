# Plan: Update Old LPFX Functions to Use Overloads

## Overview

Update the old LPFX functions (`hash`, `snoise`, `worley`) to use function overloading instead of numbered suffixes, matching the library style used by newer functions like `hsv2rgb`. This involves:

1. Changing GLSL function names from numbered variants (`lpfx_hash1`, `lpfx_snoise1`, etc.) to overloaded names (`lpfx_hash`, `lpfx_snoise`, etc.)
2. Updating GLSL signatures to use vector types (`uvec2`, `uvec3` for hash; `vec2`, `vec3` for snoise/worley)
3. Adding public Rust functions that take helper types, following lygia's naming pattern with `lpfx_` prefix (`lpfx_hash2`, `lpfx_snoise2`, etc.)
4. Updating extern C functions to call the public Rust functions
5. Regenerating the builtin registry
6. Updating test files to use new names

## Phases

1. Update hash functions to use overloads
2. Update snoise functions to use overloads
3. Update worley functions to use overloads
4. Regenerate builtin registry
5. Update test files
6. Cleanup and finalization

## Success Criteria

- All functions use overloaded names (`lpfx_hash`, `lpfx_snoise`, `lpfx_worley`, `lpfx_worley_value`)
- Hash functions use `uvec2`/`uvec3` types in GLSL signatures
- Public Rust functions exist with lygia-style naming (`lpfx_hash2`, `lpfx_snoise2`, etc.)
- Extern C functions call public Rust functions
- Builtin registry regenerated with new signatures
- All test files updated to use new names
- All tests pass
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`

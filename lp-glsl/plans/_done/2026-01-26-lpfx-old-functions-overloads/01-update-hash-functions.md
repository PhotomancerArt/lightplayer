# Phase 1: Update Hash Functions to Use Overloads

## Description

Update the hash functions in `hash.rs` to:
1. Add public Rust functions `lpfx_hash`, `lpfx_hash2`, `lpfx_hash3` following lygia's naming pattern
2. Update extern C functions to use overloaded GLSL signatures (`lpfx_hash` with `uint`/`uvec2`/`uvec3`)
3. Update extern C functions to call the new public Rust functions

## Implementation Steps

1. Extract core implementation into public Rust functions:
   - `lpfx_hash(x: u32, seed: u32) -> u32` - calls existing `hash_impl`
   - `lpfx_hash2(x: u32, y: u32, seed: u32) -> u32` - combines coordinates and calls `hash_impl`
   - `lpfx_hash3(x: u32, y: u32, z: u32, seed: u32) -> u32` - combines coordinates and calls `hash_impl`

2. Update extern C functions:
   - `__lpfx_hash_1`: Change GLSL signature to `"uint lpfx_hash(uint x, uint seed)"`, call `lpfx_hash`
   - `__lpfx_hash_2`: Change GLSL signature to `"uint lpfx_hash(uvec2 xy, uint seed)"`, call `lpfx_hash2`
   - `__lpfx_hash_3`: Change GLSL signature to `"uint lpfx_hash(uvec3 xyz, uint seed)"`, call `lpfx_hash3`

3. Update documentation comments to reflect new GLSL names

## Success Criteria

- Public Rust functions `lpfx_hash`, `lpfx_hash2`, `lpfx_hash3` exist
- Extern C functions have updated GLSL signatures using `lpfx_hash` with different parameter types
- Extern C functions call the public Rust functions
- Code compiles without warnings
- Tests still pass (they'll be updated in phase 5)

## Code Organization

- Place public Rust functions at the top
- Place extern C wrapper functions below
- Keep tests at the bottom
- Keep helper functions (`hash_impl`) at the bottom

## Formatting

- Run `cargo +nightly fmt` on `hash.rs` before committing

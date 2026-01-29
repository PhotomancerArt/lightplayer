# Phase 2: Update Simplex Noise Functions to Use Overloads

## Description

Update the simplex noise functions to:
1. Add public Rust functions `lpfx_snoise`, `lpfx_snoise2`, `lpfx_snoise3` following lygia's naming pattern
2. Extract core implementation logic into these public functions (taking `Q32`/`Vec2Q32`/`Vec3Q32`)
3. Update extern C functions to use overloaded GLSL signatures (`lpfx_snoise` with `float`/`vec2`/`vec3`)
4. Update extern C functions to call the new public Rust functions

## Implementation Steps

1. For each snoise file (`snoise1_q32.rs`, `snoise2_q32.rs`, `snoise3_q32.rs`):
   - Extract core implementation into public Rust function:
     - `snoise1_q32.rs`: `lpfx_snoise(x: Q32, seed: u32) -> Q32`
     - `snoise2_q32.rs`: `lpfx_snoise2(p: Vec2Q32, seed: u32) -> Q32`
     - `snoise3_q32.rs`: `lpfx_snoise3(p: Vec3Q32, seed: u32) -> Q32`
   - Update extern C function:
     - `snoise1_q32.rs`: Change GLSL signature to `"float lpfx_snoise(float x, uint seed)"`, call `lpfx_snoise`
     - `snoise2_q32.rs`: Change GLSL signature to `"float lpfx_snoise(vec2 p, uint seed)"`, call `lpfx_snoise2`
     - `snoise3_q32.rs`: Change GLSL signature to `"float lpfx_snoise(vec3 p, uint seed)"`, call `lpfx_snoise3`

2. Update f32 wrapper files (`snoise1_f32.rs`, `snoise2_f32.rs`, `snoise3_f32.rs`):
   - Update GLSL signatures to use `lpfx_snoise` with appropriate parameter types
   - Ensure they call the q32 versions correctly

3. Update documentation comments to reflect new GLSL names

## Success Criteria

- Public Rust functions `lpfx_snoise`, `lpfx_snoise2`, `lpfx_snoise3` exist
- Extern C functions have updated GLSL signatures using `lpfx_snoise` with different parameter types
- Extern C functions call the public Rust functions
- Code compiles without warnings
- Tests still pass (they'll be updated in phase 5)

## Code Organization

- Place public Rust functions at the top
- Place extern C wrapper functions below
- Keep tests at the bottom
- Keep helper constants at the top

## Formatting

- Run `cargo +nightly fmt` on all modified snoise files before committing

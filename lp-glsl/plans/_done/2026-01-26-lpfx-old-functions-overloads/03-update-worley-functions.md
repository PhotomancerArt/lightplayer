# Phase 3: Update Worley Noise Functions to Use Overloads

## Description

Update the worley noise functions to:
1. Add public Rust functions `lpfx_worley2`, `lpfx_worley3`, `lpfx_worley2_value`, `lpfx_worley3_value` following lygia's naming pattern
2. Extract core implementation logic into these public functions (taking `Vec2Q32`/`Vec3Q32`)
3. Update extern C functions to use overloaded GLSL signatures (`lpfx_worley`/`lpfx_worley_value` with `vec2`/`vec3`)
4. Update extern C functions to call the new public Rust functions

## Implementation Steps

1. For worley distance functions (`worley2_q32.rs`, `worley3_q32.rs`):
   - Extract core implementation into public Rust function:
     - `worley2_q32.rs`: `lpfx_worley2(p: Vec2Q32, seed: u32) -> Q32`
     - `worley3_q32.rs`: `lpfx_worley3(p: Vec3Q32, seed: u32) -> Q32`
   - Update extern C function:
     - `worley2_q32.rs`: Change GLSL signature to `"float lpfx_worley(vec2 p, uint seed)"`, call `lpfx_worley2`
     - `worley3_q32.rs`: Change GLSL signature to `"float lpfx_worley(vec3 p, uint seed)"`, call `lpfx_worley3`

2. For worley value functions (`worley2_value_q32.rs`, `worley3_value_q32.rs`):
   - Extract core implementation into public Rust function:
     - `worley2_value_q32.rs`: `lpfx_worley2_value(p: Vec2Q32, seed: u32) -> Q32`
     - `worley3_value_q32.rs`: `lpfx_worley3_value(p: Vec3Q32, seed: u32) -> Q32`
   - Update extern C function:
     - `worley2_value_q32.rs`: Change GLSL signature to `"float lpfx_worley_value(vec2 p, uint seed)"`, call `lpfx_worley2_value`
     - `worley3_value_q32.rs`: Change GLSL signature to `"float lpfx_worley_value(vec3 p, uint seed)"`, call `lpfx_worley3_value`

3. Update f32 wrapper files:
   - Update GLSL signatures to use `lpfx_worley`/`lpfx_worley_value` with appropriate parameter types
   - Ensure they call the q32 versions correctly

4. Update documentation comments to reflect new GLSL names

## Success Criteria

- Public Rust functions `lpfx_worley2`, `lpfx_worley3`, `lpfx_worley2_value`, `lpfx_worley3_value` exist
- Extern C functions have updated GLSL signatures using `lpfx_worley`/`lpfx_worley_value` with different parameter types
- Extern C functions call the public Rust functions
- Code compiles without warnings
- Tests still pass (they'll be updated in phase 5)

## Code Organization

- Place public Rust functions at the top
- Place extern C wrapper functions below
- Keep tests at the bottom
- Keep helper constants and functions at appropriate locations

## Formatting

- Run `cargo +nightly fmt` on all modified worley files before committing

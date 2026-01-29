# Phase 6: Implement fbm functions

## Description

Implement Fractal Brownian Motion (FBM) noise functions. These combine multiple octaves of noise (snoise for 2D/3D, gnoise for tilable) to create fractal patterns.

## Implementation

### File: `builtins/lpfx/generative/fbm/mod.rs` (NEW)

Create module file exporting fbm functions.

### File: `builtins/lpfx/generative/fbm/fbm2_q32.rs` (NEW)

Implement 2D FBM:

- `lpfx_fbm2(p: Vec2Q32, octaves: i32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_fbm2_q32(x: i32, y: i32, octaves: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm (matches GLSL structure):

```rust
let mut value = Q32::ZERO;  // FBM_VALUE_INITIAL
let mut amplitude = Q32::from_f32(0.5);  // FBM_AMPLITUDE_INITIAL
let mut st = p;

for _ in 0..octaves {
    value = value + amplitude * lpfx_snoise2(st, seed);
    st = st * Q32::from_f32(2.0);  // FBM_SCALE_SCALAR
    amplitude = amplitude * Q32::from_f32(0.5);  // FBM_AMPLITUDE_SCALAR
}
value
```

### File: `builtins/lpfx/generative/fbm/fbm2_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_fbm2_f32(x: f32, y: f32, octaves: i32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/fbm/fbm3_q32.rs` (NEW)

Implement 3D FBM:

- `lpfx_fbm3(p: Vec3Q32, octaves: i32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_fbm3_q32(x: i32, y: i32, z: i32, octaves: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm (matches GLSL structure):

```rust
let mut value = Q32::ZERO;  // FBM_VALUE_INITIAL
let mut amplitude = Q32::from_f32(0.5);  // FBM_AMPLITUDE_INITIAL
let mut pos = p;

for _ in 0..octaves {
    value = value + amplitude * lpfx_snoise3(pos, seed);
    pos = pos * Q32::from_f32(2.0);  // FBM_SCALE_SCALAR
    amplitude = amplitude * Q32::from_f32(0.5);  // FBM_AMPLITUDE_SCALAR
}
value
```

### File: `builtins/lpfx/generative/fbm/fbm3_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_fbm3_f32(x: f32, y: f32, z: f32, octaves: i32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/fbm/fbm3_tile_q32.rs` (NEW)

Implement 3D tilable FBM:

- `lpfx_fbm3_tile(p: Vec3Q32, tile_length: Q32, octaves: i32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_fbm3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, octaves: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm (matches GLSL structure):

```rust
const PERSISTENCE: Q32 = Q32::from_f32(0.5);
const LACUNARITY: Q32 = Q32::from_f32(2.0);

let mut amplitude = Q32::from_f32(0.5);
let mut total = Q32::ZERO;
let mut normalization = Q32::ZERO;
let mut pos = p;

for _ in 0..octaves {
    let noise_value = lpfx_gnoise3_tile(pos, tile_length * LACUNARITY * Q32::HALF, seed);
    let normalized_noise = noise_value * Q32::HALF + Q32::HALF;  // [0, 1]
    total = total + normalized_noise * amplitude;
    normalization = normalization + amplitude;
    amplitude = amplitude * PERSISTENCE;
    pos = pos * LACUNARITY;
}
total / normalization
```

### File: `builtins/lpfx/generative/fbm/fbm3_tile_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_fbm3_tile_f32(x: f32, y: f32, z: f32, tile_length: f32, octaves: i32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/mod.rs` (UPDATE)

Export `fbm` module.

## Success Criteria

- All fbm functions implemented (2D, 3D, 3D tilable)
- All functions have both q32 and f32 implementations
- Functions use `#[lpfx_impl_macro::lpfx_impl]` attribute
- Functions use `#[unsafe(no_mangle)]` and `pub extern "C"`
- Code structure matches GLSL source closely (loop structure identical)
- Code compiles without errors
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
- Use measured, factual descriptions of what was implemented

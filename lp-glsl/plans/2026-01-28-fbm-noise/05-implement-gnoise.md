# Phase 5: Implement gnoise functions

## Description

Implement gradient noise (gnoise) functions. These use random values at grid cell corners and interpolate between them using cubic (2D) or quintic (3D) interpolation.

## Implementation

### File: `builtins/lpfx/generative/gnoise/mod.rs` (NEW)

Create module file exporting gnoise functions.

### File: `builtins/lpfx/generative/gnoise/gnoise1_q32.rs` (NEW)

Implement 1D gradient noise:

- `lpfx_gnoise1(x: Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_gnoise1_q32(x: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm:

1. `i = floor(x)`, `f = fract(x)`
2. `a = random(i, seed)`, `b = random(i + 1, seed)`
3. `u = smoothstep(0.0, 1.0, f)` (or use cubic)
4. `mix(a, b, u)`

### File: `builtins/lpfx/generative/gnoise/gnoise1_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_gnoise1_f32(x: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/gnoise/gnoise2_q32.rs` (NEW)

Implement 2D gradient noise:

- `lpfx_gnoise2(p: Vec2Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_gnoise2_q32(x: i32, y: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm:

1. `i = floor(p)`, `f = fract(p)`
2. Sample corners: `a = random(i)`, `b = random(i + (1,0))`, `c = random(i + (0,1))`, `d = random(i + (1,1))`
3. `u = cubic(f)`
4. Bilinear interpolation: `mix(mix(a, b, u.x), mix(c, d, u.x), u.y)` with proper cross terms

### File: `builtins/lpfx/generative/gnoise/gnoise2_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_gnoise2_f32(x: f32, y: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/gnoise/gnoise3_q32.rs` (NEW)

Implement 3D gradient noise:

- `lpfx_gnoise3(p: Vec3Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_gnoise3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm:

1. `i = floor(p)`, `f = fract(p)`
2. Sample all 8 corners using `random()`
3. `u = quintic(f)`
4. Trilinear interpolation using nested mixes

### File: `builtins/lpfx/generative/gnoise/gnoise3_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_gnoise3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/gnoise/gnoise3_tile_q32.rs` (NEW)

Implement 3D tilable gradient noise:

- `lpfx_gnoise3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_gnoise3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm:

1. `i = floor(p)`, `f = fract(p)`
2. Sample corners using `srandom3_tile(i + offset, tile_length * lacunarity * 0.5, seed)` with dot products
3. `u = quintic(f)`
4. Trilinear interpolation
5. Normalize result to [0, 1] range: `(noise_value * 0.5 + 0.5)`

### File: `builtins/lpfx/generative/gnoise/gnoise3_tile_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_gnoise3_tile_f32(x: f32, y: f32, z: f32, tile_length: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/mod.rs` (UPDATE)

Export `gnoise` module.

## Success Criteria

- All gnoise functions implemented (1D, 2D, 3D, 3D tilable)
- All functions have both q32 and f32 implementations
- Functions use `#[lpfx_impl_macro::lpfx_impl]` attribute
- Functions use `#[unsafe(no_mangle)]` and `pub extern "C"`
- Code structure matches GLSL source closely
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

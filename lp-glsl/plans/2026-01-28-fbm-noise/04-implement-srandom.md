# Phase 4: Implement srandom functions

## Description

Implement `srandom()` (signed random) functions that return values in [-1, 1]. These are used by the tilable gnoise variant. Also implement `srandom3_vec()` that returns a Vec3Q32.

## Implementation

### File: `builtins/lpfx/generative/srandom/mod.rs` (NEW)

Create module file exporting srandom functions.

### File: `builtins/lpfx/generative/srandom/srandom1_q32.rs` (NEW)

Implement 1D signed random:

- `lpfx_srandom1(x: Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_srandom1_q32(x: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `-1.0 + 2.0 * random(x, seed)`

### File: `builtins/lpfx/generative/srandom/srandom1_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_srandom1_f32(x: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/srandom/srandom2_q32.rs` (NEW)

Implement 2D signed random:

- `lpfx_srandom2(p: Vec2Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_srandom2_q32(x: i32, y: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `-1.0 + 2.0 * random(p, seed)`

### File: `builtins/lpfx/generative/srandom/srandom2_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_srandom2_f32(x: f32, y: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/srandom/srandom3_q32.rs` (NEW)

Implement 3D signed random:

- `lpfx_srandom3(p: Vec3Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_srandom3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `-1.0 + 2.0 * random(p, seed)`

### File: `builtins/lpfx/generative/srandom/srandom3_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_srandom3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/srandom/srandom3_vec_q32.rs` (NEW)

Implement 3D signed random returning Vec3Q32:

- `lpfx_srandom3_vec(p: Vec3Q32, seed: u32) -> Vec3Q32` - Public Rust function
- `__lpfx_srandom3_vec_q32(x: i32, y: i32, z: i32, seed: u32, out: *mut i32)` - Extern C wrapper

Algorithm: Uses dot products with different constant vectors for each component:

- x: `dot(p, vec3(127.1, 311.7, 74.7))`
- y: `dot(p, vec3(269.5, 183.3, 246.1))`
- z: `dot(p, vec3(113.5, 271.9, 124.6))`
  Then: `-1.0 + 2.0 * fract(sin(dot_result) * 43758.5453123)`

### File: `builtins/lpfx/generative/srandom/srandom3_tile_q32.rs` (NEW)

Implement 3D signed random with tiling:

- `lpfx_srandom3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Vec3Q32` - Public Rust function
- `__lpfx_srandom3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, seed: u32, out: *mut i32)` - Extern C wrapper

Algorithm: `mod(p, tile_length)` then call `srandom3_vec()`

### File: `builtins/lpfx/generative/srandom/srandom3_tile_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_srandom3_tile_f32(x: f32, y: f32, z: f32, tile_length: f32, seed: u32, out: *mut f32)` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/mod.rs` (UPDATE)

Export `srandom` module.

## Success Criteria

- All srandom functions implemented (1D, 2D, 3D, 3D vec, 3D tile)
- All functions have both q32 and f32 implementations
- Functions use `#[lpfx_impl_macro::lpfx_impl]` attribute
- Functions use `#[unsafe(no_mangle)]` and `pub extern "C"`
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

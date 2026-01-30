# Phase 3: Implement random functions

## Description

Implement `random()` functions that return values in [0, 1] using sin-based hashing. These are used by gnoise to sample random values at grid cell corners.

## Implementation

### File: `builtins/lpfx/generative/random/mod.rs` (NEW)

Create module file exporting random functions.

### File: `builtins/lpfx/generative/random/random1_q32.rs` (NEW)

Implement 1D random:

- `lpfx_random1(x: Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_random1_q32(x: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `fract(sin(x + seed) * 43758.5453)`

### File: `builtins/lpfx/generative/random/random1_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_random1_f32(x: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/random/random2_q32.rs` (NEW)

Implement 2D random:

- `lpfx_random2(p: Vec2Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_random2_q32(x: i32, y: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `fract(sin(dot(p, vec2(12.9898, 78.233)) + seed) * 43758.5453)`

### File: `builtins/lpfx/generative/random/random2_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_random2_f32(x: f32, y: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/random/random3_q32.rs` (NEW)

Implement 3D random:

- `lpfx_random3(p: Vec3Q32, seed: u32) -> Q32` - Public Rust function
- `__lpfx_random3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32` - Extern C wrapper

Algorithm: `fract(sin(dot(p, vec3(70.9898, 78.233, 32.4355)) + seed) * 43758.5453123)`

### File: `builtins/lpfx/generative/random/random3_f32.rs` (NEW)

Implement f32 wrapper:

- `__lpfx_random3_f32(x: f32, y: f32, z: f32, seed: u32) -> f32` - Converts to q32, calls q32 version, converts back

### File: `builtins/lpfx/generative/mod.rs` (UPDATE)

Export `random` module.

## Success Criteria

- All random functions implemented (1D, 2D, 3D)
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

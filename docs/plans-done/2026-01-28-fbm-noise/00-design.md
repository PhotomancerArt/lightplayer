# Design: Adapt Lygia FBM Noise to LPFX Function

## Overview

Adapt the Fractal Brownian Motion (FBM) noise function from Lygia to work as an LPFX builtin
function. This includes implementing gnoise (gradient noise) and all its dependencies (random,
srandom, cubic, quintic interpolation, and mix/lerp functions) to keep the Rust code as close as
possible to the original GLSL code.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/
├── glsl/q32/
│   ├── types/
│   │   ├── q32.rs                              # UPDATE: Add mix() method
│   │   ├── vec2_q32.rs                         # UPDATE: Add mix() method
│   │   ├── vec3_q32.rs                         # UPDATE: Add mix() method
│   │   └── vec4_q32.rs                         # UPDATE: Add mix() method
│   └── fns/
│       ├── mod.rs                              # UPDATE: Export new functions
│       ├── mix.rs                              # NEW: mix() standalone functions
│       ├── cubic.rs                            # NEW: cubic() interpolation functions
│       └── quintic.rs                           # NEW: quintic() interpolation functions
└── builtins/lpfx/
    ├── generative/
    │   ├── mod.rs                              # UPDATE: Export gnoise and fbm modules
    │   ├── random/
    │   │   ├── mod.rs                          # NEW: Random functions module
    │   │   ├── random1_q32.rs                  # NEW: 1D random function
    │   │   ├── random1_f32.rs                 # NEW: 1D random f32 wrapper
    │   │   ├── random2_q32.rs                  # NEW: 2D random function
    │   │   ├── random2_f32.rs                  # NEW: 2D random f32 wrapper
    │   │   ├── random3_q32.rs                  # NEW: 3D random function
    │   │   └── random3_f32.rs                  # NEW: 3D random f32 wrapper
    │   ├── srandom/
    │   │   ├── mod.rs                          # NEW: Signed random functions module
    │   │   ├── srandom1_q32.rs                 # NEW: 1D signed random function
    │   │   ├── srandom1_f32.rs                # NEW: 1D signed random f32 wrapper
    │   │   ├── srandom2_q32.rs                 # NEW: 2D signed random function
    │   │   ├── srandom2_f32.rs                 # NEW: 2D signed random f32 wrapper
    │   │   ├── srandom3_q32.rs                 # NEW: 3D signed random function
    │   │   ├── srandom3_f32.rs                 # NEW: 3D signed random f32 wrapper
    │   │   ├── srandom3_tile_q32.rs            # NEW: 3D signed random with tiling
    │   │   └── srandom3_tile_f32.rs            # NEW: 3D signed random with tiling f32 wrapper
    │   ├── gnoise/
    │   │   ├── mod.rs                          # NEW: Gradient noise module
    │   │   ├── gnoise1_q32.rs                  # NEW: 1D gradient noise
    │   │   ├── gnoise1_f32.rs                  # NEW: 1D gradient noise f32 wrapper
    │   │   ├── gnoise2_q32.rs                  # NEW: 2D gradient noise
    │   │   ├── gnoise2_f32.rs                  # NEW: 2D gradient noise f32 wrapper
    │   │   ├── gnoise3_q32.rs                  # NEW: 3D gradient noise
    │   │   ├── gnoise3_f32.rs                  # NEW: 3D gradient noise f32 wrapper
    │   │   ├── gnoise3_tile_q32.rs             # NEW: 3D tilable gradient noise
    │   │   └── gnoise3_tile_f32.rs             # NEW: 3D tilable gradient noise f32 wrapper
    │   └── fbm/
    │       ├── mod.rs                           # NEW: FBM noise module
    │       ├── fbm2_q32.rs                      # NEW: 2D FBM noise
    │       ├── fbm2_f32.rs                      # NEW: 2D FBM noise f32 wrapper
    │       ├── fbm3_q32.rs                      # NEW: 3D FBM noise
    │       ├── fbm3_f32.rs                      # NEW: 3D FBM noise f32 wrapper
    │       ├── fbm3_tile_q32.rs                 # NEW: 3D tilable FBM noise
    │       └── fbm3_tile_f32.rs                 # NEW: 3D tilable FBM noise f32 wrapper
    └── mod.rs                                   # UPDATE: Export new modules
```

## Types Summary

### Mix/Lerp Functions (`glsl/q32/fns/mix.rs`)

```
mix() - Linear interpolation
├── mix_q32(a: Q32, b: Q32, t: Q32) -> Q32 - # NEW: Scalar mix
├── mix_vec2(a: Vec2Q32, b: Vec2Q32, t: Vec2Q32) -> Vec2Q32 - # NEW: Vec2 mix
├── mix_vec3(a: Vec3Q32, b: Vec3Q32, t: Vec3Q32) -> Vec3Q32 - # NEW: Vec3 mix
└── mix_vec4(a: Vec4Q32, b: Vec4Q32, t: Vec4Q32) -> Vec4Q32 - # NEW: Vec4 mix
```

### Q32 and Vector Type Updates

```
Q32 - # UPDATE: Add mix() method
└── mix(self, other: Q32, t: Q32) -> Q32 - # NEW: Linear interpolation

Vec2Q32 - # UPDATE: Add mix() method
└── mix(self, other: Vec2Q32, t: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise mix

Vec3Q32 - # UPDATE: Add mix() method
└── mix(self, other: Vec3Q32, t: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise mix

Vec4Q32 - # UPDATE: Add mix() method
└── mix(self, other: Vec4Q32, t: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise mix
```

### Cubic Interpolation (`glsl/q32/fns/cubic.rs`)

```
cubic() - Cubic polynomial smoothing (3t² - 2t³)
├── cubic_q32(v: Q32) -> Q32 - # NEW: Scalar cubic
├── cubic_vec2(v: Vec2Q32) -> Vec2Q32 - # NEW: Vec2 cubic
├── cubic_vec3(v: Vec3Q32) -> Vec3Q32 - # NEW: Vec3 cubic
└── cubic_vec4(v: Vec4Q32) -> Vec4Q32 - # NEW: Vec4 cubic
```

### Quintic Interpolation (`glsl/q32/fns/quintic.rs`)

```
quintic() - Quintic polynomial smoothing (6t⁵ - 15t⁴ + 10t³)
├── quintic_q32(v: Q32) -> Q32 - # NEW: Scalar quintic
├── quintic_vec2(v: Vec2Q32) -> Vec2Q32 - # NEW: Vec2 quintic
├── quintic_vec3(v: Vec3Q32) -> Vec3Q32 - # NEW: Vec3 quintic
└── quintic_vec4(v: Vec4Q32) -> Vec4Q32 - # NEW: Vec4 quintic
```

### Random Functions (`builtins/lpfx/generative/random/`)

```
random() - Returns [0, 1] using sin-based hash
├── lpfx_random1(x: Q32, seed: u32) -> Q32 - # NEW: 1D random
├── lpfx_random2(p: Vec2Q32, seed: u32) -> Q32 - # NEW: 2D random
└── lpfx_random3(p: Vec3Q32, seed: u32) -> Q32 - # NEW: 3D random

__lpfx_random1_q32(x: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_random2_q32(x: i32, y: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_random3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
```

### Signed Random Functions (`builtins/lpfx/generative/srandom/`)

```
srandom() - Returns [-1, 1] (signed random)
├── lpfx_srandom1(x: Q32, seed: u32) -> Q32 - # NEW: 1D signed random
├── lpfx_srandom2(p: Vec2Q32, seed: u32) -> Q32 - # NEW: 2D signed random
├── lpfx_srandom3(p: Vec3Q32, seed: u32) -> Q32 - # NEW: 3D signed random
└── lpfx_srandom3_vec(p: Vec3Q32, seed: u32) -> Vec3Q32 - # NEW: 3D signed random vec3

__lpfx_srandom1_q32(x: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_srandom2_q32(x: i32, y: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_srandom3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_srandom3_vec_q32(x: i32, y: i32, z: i32, seed: u32, out: *mut i32) - # NEW: Extern C wrapper (vec3 output)

srandom3_tile() - Signed random with tiling
├── lpfx_srandom3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Vec3Q32 - # NEW: 3D signed random with tiling
└── __lpfx_srandom3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, seed: u32, out: *mut i32) - # NEW: Extern C wrapper
```

### Gradient Noise Functions (`builtins/lpfx/generative/gnoise/`)

```
gnoise() - Gradient noise (Perlin-style)
├── lpfx_gnoise1(x: Q32, seed: u32) -> Q32 - # NEW: 1D gradient noise
├── lpfx_gnoise2(p: Vec2Q32, seed: u32) -> Q32 - # NEW: 2D gradient noise
├── lpfx_gnoise3(p: Vec3Q32, seed: u32) -> Q32 - # NEW: 3D gradient noise
└── lpfx_gnoise3_tile(p: Vec3Q32, tile_length: Q32, seed: u32) -> Q32 - # NEW: 3D tilable gradient noise

__lpfx_gnoise1_q32(x: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_gnoise2_q32(x: i32, y: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_gnoise3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_gnoise3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
```

### FBM Noise Functions (`builtins/lpfx/generative/fbm/`)

```
fbm() - Fractal Brownian Motion noise
├── lpfx_fbm2(p: Vec2Q32, octaves: i32, seed: u32) -> Q32 - # NEW: 2D FBM
├── lpfx_fbm3(p: Vec3Q32, octaves: i32, seed: u32) -> Q32 - # NEW: 3D FBM
└── lpfx_fbm3_tile(p: Vec3Q32, tile_length: Q32, octaves: i32, seed: u32) -> Q32 - # NEW: 3D tilable FBM

__lpfx_fbm2_q32(x: i32, y: i32, octaves: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_fbm3_q32(x: i32, y: i32, z: i32, octaves: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
__lpfx_fbm3_tile_q32(x: i32, y: i32, z: i32, tile_length: i32, octaves: i32, seed: u32) -> i32 - # NEW: Extern C wrapper
```

## Implementation Notes

### GLSL to Rust Translation Strategy

Keep the Rust code structure as close as possible to the GLSL source:

1. **Loop Structure**: Match the GLSL loop structure exactly

   ```glsl
   for (int i = 0; i < FBM_OCTAVES; i++) {
       value += amplitude * FBM_NOISE2_FNC(st);
       st *= FBM_SCALE_SCALAR;
       amplitude *= FBM_AMPLITUDE_SCALAR;
   }
   ```

   Becomes:

   ```rust
   for i in 0..octaves {
       value = value + amplitude * noise_fn(st, seed);
       st = st * SCALE_SCALAR;
       amplitude = amplitude * AMPLITUDE_SCALAR;
   }
   ```

2. **Constants**: Use constants matching GLSL defaults:
    - `FBM_VALUE_INITIAL = 0.0`
    - `FBM_SCALE_SCALAR = 2.0` (lacunarity)
    - `FBM_AMPLITUDE_INITIAL = 0.5`
    - `FBM_AMPLITUDE_SCALAR = 0.5` (persistence)

3. **Helper Functions**: Create helper functions that mirror GLSL function calls:
    - `random()` → `lpfx_random()`
    - `srandom3()` → `lpfx_srandom3_vec()`
    - `cubic()` → `cubic()`
    - `quintic()` → `quintic()`
    - `mix()` → `mix()`

### Random Function Implementation

The GLSL `random()` function uses sin-based hashing:

```glsl
float random(in float x) {
    return fract(sin(x) * 43758.5453);
}
```

For multi-dimensional versions, it uses dot products with constant vectors:

```glsl
float random(in vec2 st) {
    return fract(sin(dot(st.xy, vec2(12.9898, 78.233))) * 43758.5453);
}
```

We'll implement these using `__lp_q32_sin` and Q32 arithmetic.

### Gradient Noise Algorithm

Gradient noise (gnoise) works by:

1. Dividing space into integer grid cells
2. Sampling random values at cell corners using `random()`
3. Interpolating between corners using `cubic()` (2D) or `quintic()` (3D)
4. For tilable version, uses `srandom3()` with `mod()` for seamless tiling

### FBM Algorithm

FBM combines multiple octaves of noise:

1. Start with initial value and amplitude
2. For each octave:
    - Add `amplitude * noise(position)` to value
    - Scale position by lacunarity (2.0)
    - Scale amplitude by persistence (0.5)
3. Tilable variant normalizes by accumulated amplitude

### Function Pattern

All functions follow the standard LPFX pattern:

- Public Rust function: `lpfx_*` with nice types (Q32, Vec2Q32, etc.)
- Extern C wrapper: `__lpfx_*` with expanded types (i32, flattened vectors)
- F32 wrapper: `__lpfx_*_f32` that converts to q32, calls q32 version, converts back

## Success Criteria

1. All helper functions (mix, cubic, quintic) implemented and tested
2. All random functions (random, srandom) implemented and tested
3. All gnoise functions implemented and tested
4. All fbm functions implemented and tested
5. Code structure matches GLSL source closely
6. All functions have both q32 and f32 implementations
7. All functions registered in builtin system
8. Code formatted with `cargo +nightly fmt`
9. All tests pass

# Design: Update Old LPFX Functions to Use Overloads

## Overview

Update the old LPFX functions (`hash`, `snoise`, `worley`) to use function overloading instead of numbered suffixes, matching the library style used by newer functions like `hsv2rgb`. This involves:

1. Changing GLSL function names from numbered variants (`lpfx_hash1`, `lpfx_snoise1`, etc.) to overloaded names (`lpfx_hash`, `lpfx_snoise`, etc.)
2. Updating GLSL signatures to use vector types (`uvec2`, `uvec3` for hash; `vec2`, `vec3` for snoise/worley)
3. Adding public Rust functions that take helper types (following the pattern from `hsv2rgb`)
4. Updating extern C functions to call the public Rust functions
5. Regenerating the builtin registry
6. Updating test files to use new names

## Architecture

### File Structure

```
lp-glsl/crates/lp-builtins/src/builtins/lpfx/
├── hash.rs                              # UPDATE: Add public Rust functions, update GLSL signatures
├── generative/
│   ├── snoise/
│   │   ├── snoise1_q32.rs               # UPDATE: Add public Rust function, update GLSL signature
│   │   ├── snoise1_f32.rs               # UPDATE: Update GLSL signature
│   │   ├── snoise2_q32.rs               # UPDATE: Add public Rust function, update GLSL signature
│   │   ├── snoise2_f32.rs               # UPDATE: Update GLSL signature
│   │   ├── snoise3_q32.rs               # UPDATE: Add public Rust function, update GLSL signature
│   │   └── snoise3_f32.rs               # UPDATE: Update GLSL signature
│   └── worley/
│       ├── worley2_q32.rs               # UPDATE: Add public Rust function, update GLSL signature
│       ├── worley2_f32.rs               # UPDATE: Update GLSL signature
│       ├── worley3_q32.rs               # UPDATE: Add public Rust function, update GLSL signature
│       ├── worley3_f32.rs               # UPDATE: Update GLSL signature
│       ├── worley2_value_q32.rs         # UPDATE: Add public Rust function, update GLSL signature
│       ├── worley2_value_f32.rs         # UPDATE: Update GLSL signature
│       ├── worley3_value_q32.rs         # UPDATE: Add public Rust function, update GLSL signature
│       └── worley3_value_f32.rs         # UPDATE: Update GLSL signature

lp-glsl/crates/lp-glsl-filetests/filetests/lpfx/
├── lp_hash.glsl                         # UPDATE: Change lpfx_hash1/2/3 to lpfx_hash
├── lp_simplex1.glsl                     # UPDATE: Change lpfx_snoise1 to lpfx_snoise
├── lp_simplex2.glsl                     # UPDATE: Change lpfx_snoise2 to lpfx_snoise
└── lp_simplex3.glsl                     # UPDATE: Change lpfx_snoise3 to lpfx_snoise
```

### Types and Functions

#### Hash Functions (`hash.rs`)

Following lygia's naming pattern with `lpfx_` prefix (no dimension suffix for 1D, dimension number for multi-D):
```
lpfx_hash(x: u32, seed: u32) -> u32
  # NEW: Public Rust function for 1D hash (matches lygia's pattern - no number for 1D)

lpfx_hash2(x: u32, y: u32, seed: u32) -> u32
  # NEW: Public Rust function for 2D hash (matches lygia: random2, snoise2, etc.)

lpfx_hash3(x: u32, y: u32, z: u32, seed: u32) -> u32
  # NEW: Public Rust function for 3D hash (matches lygia: random3, snoise3, etc.)

__lpfx_hash_1(x: u32, seed: u32) -> u32
  # UPDATE: GLSL signature: "uint lpfx_hash(uint x, uint seed)"
  # Calls lpfx_hash(x, seed)

__lpfx_hash_2(x: u32, y: u32, seed: u32) -> u32
  # UPDATE: GLSL signature: "uint lpfx_hash(uvec2 xy, uint seed)"
  # Calls lpfx_hash2(x, y, seed)

__lpfx_hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32
  # UPDATE: GLSL signature: "uint lpfx_hash(uvec3 xyz, uint seed)"
  # Calls lpfx_hash3(x, y, z, seed)
```

#### Simplex Noise Functions (`snoise/*.rs`)

Following lygia's naming with `lpfx_` prefix: `lpfx_snoise2`, `lpfx_snoise3`:
```
lpfx_snoise(x: Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 1D snoise (lygia doesn't have 1D, but we keep it)

lpfx_snoise2(p: Vec2Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 2D snoise (matches lygia: snoise2)

lpfx_snoise3(p: Vec3Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 3D snoise (matches lygia: snoise3)

__lpfx_snoise1_q32(x: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_snoise(float x, uint seed)"
  # Calls lpfx_snoise(Q32::from_fixed(x), seed).to_fixed()

__lpfx_snoise2_q32(x: i32, y: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_snoise(vec2 p, uint seed)"
  # Calls lpfx_snoise2(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()

__lpfx_snoise3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_snoise(vec3 p, uint seed)"
  # Calls lpfx_snoise3(Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)), seed).to_fixed()
```

#### Worley Noise Functions (`worley/*.rs`)

Following lygia's naming with `lpfx_` prefix: `lpfx_worley2`, `lpfx_worley3`:
```
lpfx_worley2(p: Vec2Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 2D worley distance (matches lygia: worley2)

lpfx_worley3(p: Vec3Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 3D worley distance (matches lygia: worley3)

# Note: Our "value" variant returns a hash value, not distance
lpfx_worley2_value(p: Vec2Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 2D worley value (hash-based)

lpfx_worley3_value(p: Vec3Q32, seed: u32) -> Q32
  # NEW: Public Rust function for 3D worley value (hash-based)

__lpfx_worley2_q32(x: i32, y: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_worley(vec2 p, uint seed)"
  # Calls lpfx_worley2(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()

__lpfx_worley3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_worley(vec3 p, uint seed)"
  # Calls lpfx_worley3(Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)), seed).to_fixed()

__lpfx_worley2_value_q32(x: i32, y: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_worley_value(vec2 p, uint seed)"
  # Calls lpfx_worley2_value(Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y)), seed).to_fixed()

__lpfx_worley3_value_q32(x: i32, y: i32, z: i32, seed: u32) -> i32
  # UPDATE: GLSL signature: "float lpfx_worley_value(vec3 p, uint seed)"
  # Calls lpfx_worley3_value(Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z)), seed).to_fixed()
```

## Design Decisions

### 1. Hash Function Signatures

Use `uvec2` and `uvec3` types in GLSL signatures instead of multiple `uint` parameters. This matches the pattern used by other vector-based functions and makes the API cleaner.

### 2. Public Rust Functions

Add public Rust functions that take helper types (`Vec2Q32`, `Vec3Q32` for snoise/worley; individual `u32` params for hash since no unsigned vector types exist). These functions contain the core implementation logic.

**Naming Convention:** Match lygia's WGSL naming pattern with `lpfx_` prefix:
- `lpfx_snoise`, `lpfx_snoise2`, `lpfx_snoise3` (dimension number suffix)
- `lpfx_worley2`, `lpfx_worley3` (dimension number suffix)
- `lpfx_worley2_value`, `lpfx_worley3_value` (value variants)
- `lpfx_hash`, `lpfx_hash2`, `lpfx_hash3` (no suffix for 1D, dimension number for multi-D)

### 3. Extern C Functions

Extern C functions receive expanded types (individual `u32`/`i32` parameters) from the compiler. They convert to helper types, call the public Rust functions, and convert results back.

### 4. Internal Function Names

Keep internal function names (`__lpfx_hash_1`, `__lpfx_snoise1_q32`, etc.) unchanged. These are implementation details and are mapped by the builtin registry.

### 5. Backward Compatibility

No backward compatibility - remove old function names entirely. Test files will be updated to use new names.

### 6. Code Organization

Follow the pattern from `hsv2rgb`:
- Public Rust functions at the top
- Extern C wrapper functions below
- Tests at the bottom

## Implementation Notes

### Hash Function Structure

For hash functions, since there are no unsigned vector helper types, the public Rust functions will take individual `u32` parameters. The extern C functions will extract components from the expanded parameters (which come from `uvec2`/`uvec3` in GLSL).

### Simplex/Worley Function Structure

For snoise and worley functions, extract the core implementation logic into public Rust functions that take `Vec2Q32`/`Vec3Q32`. The extern C functions will construct these vectors from expanded parameters and call the public functions.

### Test File Updates

Update all test files to use the new overloaded names:
- `lpfx_hash1` → `lpfx_hash` (with `uint` parameter)
- `lpfx_hash2` → `lpfx_hash` (with `uvec2` parameter)
- `lpfx_hash3` → `lpfx_hash` (with `uvec3` parameter)
- `lpfx_snoise1` → `lpfx_snoise` (with `float` parameter)
- `lpfx_snoise2` → `lpfx_snoise` (with `vec2` parameter)
- `lpfx_snoise3` → `lpfx_snoise` (with `vec3` parameter)
- `lpfx_worley2` → `lpfx_worley` (with `vec2` parameter)
- `lpfx_worley3` → `lpfx_worley` (with `vec3` parameter)
- `lpfx_worley2_value` → `lpfx_worley_value` (with `vec2` parameter)
- `lpfx_worley3_value` → `lpfx_worley_value` (with `vec3` parameter)

## Success Criteria

- All functions use overloaded names (`lpfx_hash`, `lpfx_snoise`, `lpfx_worley`, `lpfx_worley_value`)
- Hash functions use `uvec2`/`uvec3` types in GLSL signatures
- Public Rust functions exist that take helper types
- Extern C functions call public Rust functions
- Builtin registry regenerated with new signatures
- All test files updated to use new names
- All tests pass
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`

# Design: Worley Noise Implementation

## Overview

Add Worley noise (cellular noise) functions to the LP builtin library, following the same pattern as
Simplex noise. Worley noise generates cellular patterns based on the distance to the nearest feature
point in a grid.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/builtins/lpfx/
├── hash.rs                    # EXISTING: Hash functions
├── simplex/                    # EXISTING: Simplex noise functions
│   └── ...
├── worley/                    # NEW: Worley noise functions
│   ├── mod.rs                 # NEW: Module exports
│   ├── worley2_q32.rs         # NEW: 2D Worley distance
│   ├── worley2_value_q32.rs    # NEW: 2D Worley value
│   ├── worley3_q32.rs         # NEW: 3D Worley distance
│   └── worley3_value_q32.rs   # NEW: 3D Worley value
└── mod.rs                     # UPDATE: Add worley module export
```

## Types Summary

### Function Mapping

LP library functions map user-facing names to internal implementation functions:

```
User-facing name                    Internal symbol name (auto-registered)
lpfx_worley2(vec2 p, uint seed)     -> __lpfx_worley2_q32(i32 x, i32 y, u32 seed) -> i32
lpfx_worley2_value(vec2 p, uint seed) -> __lpfx_worley2_value_q32(i32 x, i32 y, u32 seed) -> i32
lpfx_worley3(vec3 p, uint seed)     -> __lpfx_worley3_q32(i32 x, i32 y, i32 z, u32 seed) -> i32
lpfx_worley3_value(vec3 p, uint seed) -> __lpfx_worley3_value_q32(i32 x, i32 y, i32 z, u32 seed) -> i32
```

Internal functions are automatically registered by `lp-glsl-builtin-gen-app` which scans
`lp-glsl-builtins/src/builtins/lpfx/` and adds them to the `BuiltinId` enum.

### Builtin Implementations (`lp-glsl-builtins/src/builtins/lpfx/worley/`)

```
worley2_q32.rs:
  __lpfx_worley2_q32(x: i32, y: i32, seed: u32) -> i32
    - Returns euclidean squared distance to nearest feature point
    - Range: approximately [-1, 1] (Q32 fixed-point)

worley2_value_q32.rs:
  __lpfx_worley2_value_q32(x: i32, y: i32, seed: u32) -> i32
    - Returns hash value of nearest cell
    - Range: approximately [-1, 1] (Q32 fixed-point)

worley3_q32.rs:
  __lpfx_worley3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32
    - Returns euclidean squared distance to nearest feature point
    - Range: approximately [-1, 1] (Q32 fixed-point)

worley3_value_q32.rs:
  __lpfx_worley3_value_q32(x: i32, y: i32, z: i32, seed: u32) -> i32
    - Returns hash value of nearest cell
    - Range: approximately [-1, 1] (Q32 fixed-point)
```

## Design Decisions

### 1. Integration with Builtin System

Worley noise functions follow the same pattern as Simplex noise:

- Implemented in `lp-glsl-builtins/src/builtins/lpfx/worley/` subdirectory
- Use `#[lpfx_impl_macro::lpfx_impl]` attribute for auto-registration
- Functions are automatically discovered and registered by `lp-glsl-builtin-gen-app`

### 2. Distance Function

Only euclidean squared distance is implemented:

- Fastest option (no sqrt required)
- Sufficient for most use cases
- Users can take sqrt in GLSL if actual distance is needed

### 3. Return Types

Two variants per dimension:

- Base function (`lpfx_worley2`, `lpfx_worley3`): Returns distance to nearest feature point
- Value variant (`lpfx_worley2_value`, `lpfx_worley3_value`): Returns hash value of nearest cell

This matches lygia's convention where the base function returns distance.

### 4. Return Value Range

All functions return values in approximately [-1, 1] range (Q32 fixed-point):

- Matches Simplex noise convention
- Matches lygia convention
- Easy to convert to [0, 1] if needed: `* 0.5 + 0.5`

### 5. Algorithm Reference

Reference implementation: `/Users/yona/dev/photomancer/oss/noise-rs/src/core/worley.rs`

Key components:

- Cell determination (floor coordinates)
- Near/far cell selection based on fractional coordinates
- Feature point generation using hash function
- Distance calculation (euclidean squared)
- Range optimization (only check cells within distance range)
- Final scaling to [-1, 1] range

### 6. Hash Function Usage

Uses existing `__lpfx_hash_2` and `__lpfx_hash_3` functions from `lpfx::hash` module, same as
Simplex noise.

### 7. Q32 Fixed-Point Considerations

- All coordinates and return values are Q32 (i32 with 16.16 format)
- Use Q32 arithmetic operations (from `lp-glsl-builtins/src/util/q32/q32.rs`)
- Distance calculations use fixed-point arithmetic
- Final scaling accounts for Q32 format

## Function Signatures

### GLSL User-Facing Signatures

```glsl
float lpfx_worley2(vec2 p, uint seed);
float lpfx_worley2_value(vec2 p, uint seed);
float lpfx_worley3(vec3 p, uint seed);
float lpfx_worley3_value(vec3 p, uint seed);
```

### Internal Signatures (flattened)

- `__lpfx_worley2_q32(x: i32, y: i32, seed: u32) -> i32`
- `__lpfx_worley2_value_q32(x: i32, y: i32, seed: u32) -> i32`
- `__lpfx_worley3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32`
- `__lpfx_worley3_value_q32(x: i32, y: i32, z: i32, seed: u32) -> i32`

**Return values:**

- All return Q32 fixed-point values (i32) in range approximately [-1, 1]

## Implementation Notes

### Worley Noise Algorithm

Worley noise requires:

1. Cell determination (floor coordinates)
2. Near/far cell selection (based on fractional coordinates > 0.5)
3. Feature point generation using hash function (call `__lpfx_hash_*` functions)
4. Distance calculation (euclidean squared)
5. Range optimization (only check cells within distance range)
6. Scaling to [-1, 1] range

Reference implementation: noise-rs `worley_2d` and `worley_3d` functions.

### Distance vs Value

- **Distance**: Returns the euclidean squared distance to the nearest feature point. Creates the
  characteristic cellular pattern.
- **Value**: Returns a hash value (normalized to [0, 1], then scaled to [-1, 1]) based on the
  nearest cell's coordinates. Useful for assigning random properties to cells.

### Testing Strategy

- Add `noise` crate as test-only dependency (if not already present)
- Compare Q32 fixed-point outputs against noise-rs f64 outputs
- Convert between formats for comparison
- Test properties: output range, continuity, seed determinism
- Test that distance and value variants produce different outputs

## Integration Points

### Module Declaration

Worley noise functions are declared as builtins via the existing builtin system. The internal
`__lpfx_worley*` functions are registered automatically by `lp-glsl-builtin-gen-app` which scans the
`lpfx/worley/` directory.

### Function Call Codegen

When emitting a function call:

1. Check if name starts with `lpfx_`
2. Lookup `LpLibFnId` by name (auto-generated from function attributes)
3. Flatten vector arguments to scalars
4. Get `FuncRef` from module
5. Generate function call instruction

### Symbol Linking

For JIT: Function pointers registered via `symbol_lookup_fn`
For Emulator: Symbols resolved by linker when linking static library

Both use `Linkage::Import` like internal builtins.

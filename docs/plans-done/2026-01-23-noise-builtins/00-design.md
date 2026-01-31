# Design: LP Library Functions for Noise Generation

## Overview

Add Lightplayer-specific library functions for noise generation that can be called from GLSL
shaders. These functions provide a standard library for shader programming, similar to GLSL builtins
but specific to Lightplayer's needs.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/builtins/q32/
├── lpfx_hash.rs                 # NEW: Hash function (1D, 2D, 3D overloads)
├── lpfx_snoise1.rs             # NEW: 1D Simplex noise
├── lpfx_snoise2.rs             # NEW: 2D Simplex noise
├── lpfx_snoise3.rs             # NEW: 3D Simplex noise
└── mod.rs                     # UPDATE: Add exports for new functions

lp-glsl/lp-glsl-compiler/src/
├── frontend/
│   ├── semantic/
│   │   └── lp_lib_fns.rs      # NEW: Semantic checking for lp_* functions
│   └── codegen/
│       └── lp_lib_fns.rs      # NEW: Codegen for LP library function calls
```

## Types Summary

### Function Mapping

LP library functions map user-facing names to internal implementation functions:

```
User-facing name          Internal symbol name (auto-registered)
lpfx_hash(u32)            -> __lpfx_hash_1(u32, u32) -> u32
lpfx_hash(u32, u32)       -> __lpfx_hash_2(u32, u32, u32) -> u32
lpfx_hash(u32, u32, u32)  -> __lpfx_hash_3(u32, u32, u32, u32) -> u32
lpfx_snoise1(i32, u32)   -> __lpfx_snoise1(i32, u32) -> i32
lpfx_snoise2(i32, i32, u32) -> __lpfx_snoise2(i32, i32, u32) -> i32
lpfx_snoise3(i32, i32, i32, u32) -> __lpfx_snoise3(i32, i32, i32, u32) -> i32
```

Internal functions (`__lp_*`) are automatically registered by `lp-glsl-builtin-gen-app` which scans
`lp-glsl-builtins/src/builtins/q32/` and adds them to the `BuiltinId` enum. The builtin generator
will
create enum variants like `LpHash1`, `LpSimplex1`, etc.

### Frontend Semantic (`frontend/semantic/lp_lib_fns.rs`)

```
LpLibFnSignature - # NEW: Function signature for LP library functions
├── name: &'static str
├── param_types: Vec<Type>
└── return_type: Type

is_lp_lib_fn(name: &str) -> bool - # NEW: Check if name starts with lp_
lookup_lp_lib_fn(name: &str) -> Option<Vec<LpLibFnSignature>> - # NEW: Lookup signatures
check_lp_lib_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String> - # NEW: Type check
```

### Frontend Codegen (`frontend/codegen/lp_lib_fns.rs`)

```
emit_lp_lib_fn_call() - # NEW: Generate code for LP library function call
├── Lookup LpLibFnId by name
├── Flatten vector arguments to individual components
├── Get FuncRef from module
└── Generate function call instruction
```

### Builtin Implementations (`lp-glsl-builtins/src/builtins/q32/`)

```
lpfx_hash.rs:
  __lpfx_hash_1(x: u32, seed: u32) -> u32 - # NEW: 1D hash
  __lpfx_hash_2(x: u32, y: u32, seed: u32) -> u32 - # NEW: 2D hash
  __lpfx_hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32 - # NEW: 3D hash

lpfx_snoise1.rs:
  __lpfx_snoise1(x: i32, seed: u32) -> i32 - # NEW: 1D Simplex noise

lpfx_snoise2.rs:
  __lpfx_snoise2(x: i32, y: i32, seed: u32) -> i32 - # NEW: 2D Simplex noise

lpfx_snoise3.rs:
  __lpfx_snoise3(x: i32, y: i32, z: i32, seed: u32) -> i32 - # NEW: 3D Simplex noise
```

## Design Decisions

### 1. Integration with Builtin System

LP library functions are implemented as internal builtins (`__lp_*` functions) in
`lp-glsl-builtins/src/builtins/q32/`, similar to existing `__lp_q32_*` functions. The user-facing
`lp_*`
names are mapped to these internal implementations during semantic checking and codegen.

### 2. Function Routing Order

Function calls are checked in this order:

1. Type constructors (vec2, mat3, etc.)
2. GLSL builtins (`is_builtin_function()`)
3. **LP library functions** (`is_lp_lib_fn()`) - **NEW**
4. User-defined functions

### 3. Vector Argument Handling

Vector arguments are flattened to individual scalar parameters:

- `lpfx_snoise2(vec2 p, uint seed)` becomes `lpfx_snoise2(i32 x, i32 y, u32 seed)`
- `lpfx_snoise3(vec3 p, uint seed)` becomes `lpfx_snoise3(i32 x, i32 y, i32 z, u32 seed)`

This matches how the compiler currently handles vectors and simplifies the implementation.

### 4. Function Signatures

Function signatures are manually specified (no auto-generation initially). Each function has:

- User-facing name: `lpfx_hash`, `lpfx_snoise1`, etc.
- Internal symbol name: `__lpfx_hash_1`, `__lpfx_snoise1`, etc.
- Parameter types: Flattened scalar types (i32, u32)
- Return type: i32 (Q32 fixed-point) or u32 (for hash)

### 5. Hash Function Algorithm

Uses the noiz hash algorithm (with attribution):

- Bit rotations, XOR operations
- Multiplication by prime 249,222,277
- Inspired by https://nullprogram.com/blog/2018/07/31/
- Optimized for noise generation quality

### 6. Simplex Noise Algorithm

Implements Simplex noise (not Perlin):

- Better quality (less directional artifacts)
- Faster in 3D (interpolates 4 corners vs 8)
- More isotropic results
- Requires skew/unskew math (more complex but worth it)

### 7. No Frequency Parameter

Frequency parameter removed - caller can scale coordinates themselves:

```glsl
lpfx_snoise3(p.x * freq, p.y * freq, p.z * freq, seed)
```

This simplifies the API and gives callers more flexibility.

### 8. Seed Parameter

Seed is XORed into the hash computation, matching noiz's approach. This provides deterministic
randomness control.

## Function Signatures

### Hash Functions

```glsl
uint lpfx_hash(uint x);
uint lpfx_hash(uint x, uint y);
uint lpfx_hash(uint x, uint y, uint z);
```

**Internal signatures:**

- `__lpfx_hash_1(x: u32, seed: u32) -> u32`
- `__lpfx_hash_2(x: u32, y: u32, seed: u32) -> u32`
- `__lpfx_hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32`

### Simplex Noise Functions

```glsl
float lpfx_snoise1(float x, uint seed);
float lpfx_snoise2(vec2 p, uint seed);
float lpfx_snoise3(vec3 p, uint seed);
```

**Internal signatures (flattened):**

- `__lpfx_snoise1(x: i32, seed: u32) -> i32`
- `__lpfx_snoise2(x: i32, y: i32, seed: u32) -> i32`
- `__lpfx_snoise3(x: i32, y: i32, z: i32, seed: u32) -> i32`

**Return values:**

- All return Q32 fixed-point values (i32) in range approximately [-1, 1]
- Hash functions return u32 values

## Implementation Notes

### Hash Function Implementation

The hash function uses the noiz algorithm:

```rust
// Inspired by https://nullprogram.com/blog/2018/07/31/
// Credit: noiz library (github.com/ElliottjPierce/noiz)
const KEY: u32 = 249_222_277; // Large prime with even bit distribution

let mut x = input;
x ^= x.rotate_right(17);
x = x.wrapping_mul(KEY);
x ^= x.rotate_right(11) ^ seed;
x = x.wrapping_mul(!KEY);
x
```

### Simplex Noise Implementation

Simplex noise requires:

1. Skew/unskew factors for coordinate transformation
2. Simplex cell determination
3. Gradient selection using hash function
4. Smooth interpolation (quintic curve)
5. Scaling to [-1, 1] range

Reference implementation: noise-rs `simplex_2d` and `simplex_3d` functions.

### Testing Strategy

- Add `noise` crate as test-only dependency
- Compare Q32 fixed-point outputs against noise-rs f64 outputs
- Convert between formats for comparison
- Test properties: output range, continuity, seed determinism

## Integration Points

### Module Declaration

LP library functions are declared as builtins via the existing builtin system. The internal `__lp_*`
functions are registered like other builtins (via `BuiltinId` enum or similar mechanism used by the
builtin generator).

### Function Call Codegen

When emitting a function call:

1. Check if name starts with `lp_`
2. Lookup `LpLibFnId` by name
3. Flatten vector arguments to scalars
4. Get `FuncRef` from module
5. Generate call instruction

### Symbol Linking

For JIT: Function pointers registered via `symbol_lookup_fn`
For Emulator: Symbols resolved by linker when linking static library

Both use `Linkage::Import` like internal builtins.

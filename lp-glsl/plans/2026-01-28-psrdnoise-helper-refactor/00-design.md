# Design: Convert psrdnoise functions to use vector helpers and add missing functions

## Overview

Refactor psrdnoise implementations to use vector helper types (Vec2Q32, Vec3Q32, Vec4Q32) instead of manually expanded component operations. Add missing GLSL-style functions to make the Rust code match the original GLSL as closely as possible.

## File Structure

```
lp-glsl/crates/lp-builtins/src/
├── glsl/q32/
│   ├── fns/                                    # UPDATE: Add standalone functions
│   │   ├── mod.rs                              # UPDATE: Export new functions
│   │   ├── floor.rs                            # NEW: floor() for vectors
│   │   ├── fract.rs                            # NEW: fract() for vectors
│   │   ├── step.rs                             # NEW: step() for vectors
│   │   ├── min.rs                              # NEW: min() for vectors
│   │   ├── max.rs                              # NEW: max() for vectors
│   │   ├── mod.rs                              # NEW: mod() for vectors
│   │   ├── sin.rs                              # NEW: sin() for vectors
│   │   ├── cos.rs                              # NEW: cos() for vectors
│   │   └── sqrt.rs                             # NEW: sqrt() for vectors
│   ├── types/
│   │   ├── vec2_q32.rs                         # UPDATE: Add floor(), fract(), step(), min(), max(), extended swizzles
│   │   ├── vec3_q32.rs                         # UPDATE: Add floor(), fract(), step(), min(), max(), extended swizzles (.xyx, .yzz)
│   │   ├── vec4_q32.rs                         # UPDATE: Add floor(), fract(), step(), min(), max(), mod(), from_vec3_scalar(), extended swizzles
│   │   └── q32.rs                              # EXISTING: Base Q32 type
│   └── mod.rs                                  # UPDATE: Export fns module
└── builtins/lpfx/generative/psrdnoise/
    ├── psrdnoise2_q32.rs                       # UPDATE: Refactor to use vector helpers
    └── psrdnoise3_q32.rs                       # UPDATE: Refactor to use vector helpers
```

## Types Summary

### Standalone Functions (`glsl/q32/fns/`)

```
floor() - Component-wise floor for Vec2/3/4
├── floor_vec2(v: Vec2Q32) -> Vec2Q32
├── floor_vec3(v: Vec3Q32) -> Vec3Q32
└── floor_vec4(v: Vec4Q32) -> Vec4Q32

fract() - Component-wise fractional part for Vec2/3/4
├── fract_vec2(v: Vec2Q32) -> Vec2Q32
├── fract_vec3(v: Vec3Q32) -> Vec3Q32
└── fract_vec4(v: Vec4Q32) -> Vec4Q32

step() - Component-wise step function for Vec2/3/4
├── step_vec2(edge: Vec2Q32, x: Vec2Q32) -> Vec2Q32
├── step_vec3(edge: Vec3Q32, x: Vec3Q32) -> Vec3Q32
└── step_vec4(edge: Vec4Q32, x: Vec4Q32) -> Vec4Q32

min() - Component-wise minimum for Vec2/3/4
├── min_vec2(a: Vec2Q32, b: Vec2Q32) -> Vec2Q32
├── min_vec3(a: Vec3Q32, b: Vec3Q32) -> Vec3Q32
└── min_vec4(a: Vec4Q32, b: Vec4Q32) -> Vec4Q32

max() - Component-wise maximum for Vec2/3/4
├── max_vec2(a: Vec2Q32, b: Vec2Q32) -> Vec2Q32
├── max_vec3(a: Vec3Q32, b: Vec3Q32) -> Vec3Q32
└── max_vec4(a: Vec4Q32, b: Vec4Q32) -> Vec4Q32

mod() - Component-wise modulo for Vec2/3/4
├── mod_vec2(x: Vec2Q32, y: Vec2Q32) -> Vec2Q32
├── mod_vec3(x: Vec3Q32, y: Vec3Q32) -> Vec3Q32
├── mod_vec4(x: Vec4Q32, y: Vec4Q32) -> Vec4Q32
├── mod_vec4_scalar(x: Vec4Q32, y: Q32) -> Vec4Q32
└── mod_vec3_scalar(x: Vec3Q32, y: Q32) -> Vec3Q32

sin() - Component-wise sine for Vec2/3/4
├── sin_vec2(v: Vec2Q32) -> Vec2Q32
├── sin_vec3(v: Vec3Q32) -> Vec3Q32
└── sin_vec4(v: Vec4Q32) -> Vec4Q32

cos() - Component-wise cosine for Vec2/3/4
├── cos_vec2(v: Vec2Q32) -> Vec2Q32
├── cos_vec3(v: Vec3Q32) -> Vec3Q32
└── cos_vec4(v: Vec4Q32) -> Vec4Q32

sqrt() - Component-wise square root for Vec2/3/4
├── sqrt_vec2(v: Vec2Q32) -> Vec2Q32
├── sqrt_vec3(v: Vec3Q32) -> Vec3Q32
└── sqrt_vec4(v: Vec4Q32) -> Vec4Q32
```

### Vec2Q32 Updates (`glsl/q32/types/vec2_q32.rs`)

```
Vec2Q32 - # UPDATE: Add new methods
├── floor(self) -> Vec2Q32 - # NEW: Component-wise floor
├── fract(self) -> Vec2Q32 - # NEW: Component-wise fractional part
├── step(self, edge: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise step
├── min(self, other: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise minimum
├── max(self, other: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise maximum
└── mod(self, other: Vec2Q32) -> Vec2Q32 - # NEW: Component-wise modulo
```

### Vec3Q32 Updates (`glsl/q32/types/vec3_q32.rs`)

```
Vec3Q32 - # UPDATE: Add new methods and swizzles
├── floor(self) -> Vec3Q32 - # NEW: Component-wise floor
├── fract(self) -> Vec3Q32 - # NEW: Component-wise fractional part
├── step(self, edge: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise step
├── min(self, other: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise minimum
├── max(self, other: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise maximum
├── mod(self, other: Vec3Q32) -> Vec3Q32 - # NEW: Component-wise modulo
├── mod_scalar(self, y: Q32) -> Vec3Q32 - # NEW: Modulo with scalar
├── xyx(self) -> Vec3Q32 - # NEW: Swizzle (x, y, x)
├── yzz(self) -> Vec3Q32 - # NEW: Swizzle (y, z, z)
└── yzx(self) -> Vec3Q32 - # NEW: Swizzle (y, z, x) - for component access
```

### Vec4Q32 Updates (`glsl/q32/types/vec4_q32.rs`)

```
Vec4Q32 - # UPDATE: Add new methods and constructors
├── floor(self) -> Vec4Q32 - # NEW: Component-wise floor
├── fract(self) -> Vec4Q32 - # NEW: Component-wise fractional part
├── step(self, edge: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise step
├── min(self, other: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise minimum
├── max(self, other: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise maximum
├── mod(self, other: Vec4Q32) -> Vec4Q32 - # NEW: Component-wise modulo
├── mod_scalar(self, y: Q32) -> Vec4Q32 - # NEW: Modulo with scalar
├── from_vec3_scalar(v: Vec3Q32, w: Q32) -> Vec4Q32 - # NEW: Constructor from Vec3 + scalar
└── xyz(self) -> Vec3Q32 - # NEW: Extract xyz as Vec3Q32 (already exists, verify)
```

## Implementation Notes

### Function Naming Convention

- Standalone functions use `_vec2`, `_vec3`, `_vec4` suffix to distinguish from scalar versions
- Methods on types use the same name as GLSL (e.g., `floor()`, `fract()`, `step()`)
- Methods delegate to standalone functions for consistency

### Performance Considerations

- All functions use `#[inline(always)]` for zero-cost abstractions
- Component-wise operations are straightforward and should inline well
- Trigonometric functions call the underlying `__lp_q32_sin`/`__lp_q32_cos` builtins

### GLSL Compatibility

- Functions match GLSL semantics exactly:
  - `floor()`: Returns largest integer <= value for each component
  - `fract()`: Returns fractional part (x - floor(x)) for each component
  - `step()`: Returns 1.0 if edge <= x, else 0.0 for each component
  - `min()`/`max()`: Component-wise minimum/maximum
  - `mod()`: Component-wise modulo

### Refactoring Strategy

1. Add helper functions first (standalone functions in `fns/`)
2. Add methods to wrapper types (delegate to standalone functions)
3. Refactor psrdnoise2_q32.rs to use helpers
4. Refactor psrdnoise3_q32.rs to use helpers
5. Verify output matches original (tests should already exist)

## Example: Before and After

### Before (psrdnoise3_q32.rs lines 177-191)

```rust
// Compute vectors to each of the simplex corners
let x0_x = x.x - v0_x;
let x0_y = x.y - v0_y;
let x0_z = x.z - v0_z;
let x1_x = x.x - v1_x;
let x1_y = x.y - v1_y;
let x1_z = x.z - v1_z;
let x2_x = x.x - v2_x;
let x2_y = x.y - v2_y;
let x2_z = x.z - v2_z;
let x3_x = x.x - v3_x;
let x3_y = x.y - v3_y;
let x3_z = x.z - v3_z;
```

### After (using vector helpers)

```rust
// Compute vectors to each of the simplex corners
let v0 = Vec3Q32::new(v0_x, v0_y, v0_z);
let v1 = Vec3Q32::new(v1_x, v1_y, v1_z);
let v2 = Vec3Q32::new(v2_x, v2_y, v2_z);
let v3 = Vec3Q32::new(v3_x, v3_y, v3_z);
let x0 = x - v0;
let x1 = x - v1;
let x2 = x - v2;
let x3 = x - v3;
```

### Before (psrdnoise3_q32.rs lines 120-122)

```rust
let g_x = if f0_x <= f0_y { Q32::ONE } else { Q32::ZERO };
let g_y = if f0_y <= f0_z { Q32::ONE } else { Q32::ZERO };
let g_z = if f0_x <= f0_z { Q32::ONE } else { Q32::ZERO };
```

### After (using step() and swizzles)

```rust
let g_ = f0.step(f0.yzx()); // step(f0.xyx, f0.yzz) equivalent
let l_ = Vec3Q32::one() - g_;
```

## Success Criteria

1. All helper functions implemented and tested
2. All wrapper type methods implemented and tested
3. psrdnoise2_q32.rs refactored to use helpers (code reduction ~30-40%)
4. psrdnoise3_q32.rs refactored to use helpers (code reduction ~30-40%)
5. All existing tests pass (no functional changes)
6. Code matches GLSL structure more closely
7. No performance regression (all functions inline)

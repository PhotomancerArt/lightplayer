# Plan: Convert psrdnoise functions to use vector helpers and add missing functions

## Questions

### Q1: Why was the code expanded out instead of using vector helpers?

**Context:** Looking at `psrdnoise3_q32.rs` lines 177-191, the code manually expands vector operations:

```rust
let x0_x = x.x - v0_x;
let x0_y = x.y - v0_y;
let x0_z = x.z - v0_z;
```

Instead of using `Vec3Q32` operations like:

```rust
let x0 = x - v0;
```

**Analysis:**

- The code was likely written before `Vec3Q32` helpers existed, or the helpers were missing key operations
- Manual expansion gives explicit control but makes code verbose and harder to match GLSL
- The GLSL code uses `vec3 x0 = x - v0;` which is much cleaner

**Suggested Answer:** The code was expanded because:

1. Vector helpers may not have existed when this was written
2. Missing operations like `floor()`, `fract()`, `step()`, `min()`, `max()` on vectors
3. Missing swizzle operations like `.xyx`, `.yzz` that GLSL uses
4. Missing `Vec4Q32` operations needed for the hash computation

### Q2: What vector helper functions are missing?

**Context:** Comparing the GLSL code (`psrdnoise.glsl`) with the Rust implementation and available helpers.

**Missing Vec3Q32 operations:**

- `floor()` - convert to integer components (currently only `to_i32()` on Q32)
- `fract()` - fractional part (currently only `frac()` on Q32)
- `step()` - step function (1.0 if edge <= x, else 0.0)
- `min()` - component-wise minimum (currently only scalar `min()`)
- `max()` - component-wise maximum (currently only scalar `max()`)
- Swizzles like `.xyx`, `.yzz` - needed for `step(f0.xyx, f0.yzz)`
- `any()` - check if any component is non-zero (for `any(greaterThan(...))`)
- `greaterThan()` - component-wise comparison returning bool vector

**Missing Vec4Q32 operations:**

- `floor()` - convert to integer components
- `mod()` - component-wise modulo
- `mod(vec4, scalar)` - modulo with scalar
- Constructor from `Vec3Q32` + scalar: `Vec4Q32::new(v3.x, v3.y, v3.z, w)`
- Swizzle operations for accessing components

**Missing Vec2Q32 operations:**

- Similar to Vec3Q32: `floor()`, `fract()`, `step()`, `min()`, `max()`

**Missing Q32 operations:**

- All trigonometric functions exist (sin, cos, sqrt)
- `mod()` exists
- `floor()` exists as `to_i32()`
- `fract()` exists as `frac()`

**Suggested Answer:** Need to add:

1. Vector `floor()` - returns vector with `to_i32()` applied to each component
2. Vector `fract()` - returns vector with `frac()` applied to each component
3. Vector `step()` - component-wise step function
4. Vector `min()` and `max()` - component-wise min/max
5. Extended swizzle operations (`.xyx`, `.yzz`, etc.)
6. `any()` and comparison helpers for vectors
7. `Vec4Q32` operations for mod, floor, etc.
8. Constructor helpers for creating Vec4 from Vec3 + scalar

### Q3: Can we rewrite the helpers to be more compact?

**Context:** Looking at the current helper implementations, they're already fairly compact. The question is whether we can add the missing operations in a clean way.

**Analysis:**

- Current helpers are well-structured with `#[inline(always)]` for performance
- Operations are component-wise, which matches GLSL semantics
- The main issue is missing operations, not verbosity of existing ones

**Suggested Answer:** The helpers are already compact. We should:

1. Add missing operations following the same pattern
2. Use consistent naming (e.g., `floor()`, `fract()` to match GLSL)
3. Keep `#[inline(always)]` for performance
4. Add swizzle operations that match GLSL patterns

### Q4: What functions are needed to match the GLSL code exactly?

**Context:** Comparing `/Users/yona/dev/photomancer/oss/lygia/generative/psrdnoise.glsl` with the Rust implementation.

**GLSL operations used:**

- `vec3 + dot(vec3, vec3) * scalar` - ✅ Available (dot exists, scalar mul exists)
- `floor(vec3)` - ❌ Missing
- `fract(vec3)` - ❌ Missing
- `step(vec3, vec3)` - ❌ Missing
- `min(vec3, vec3)` - ❌ Missing
- `max(vec3, vec3)` - ❌ Missing
- `vec3.xyx`, `vec3.yzz` swizzles - ❌ Missing
- `any(greaterThan(vec3, vec3))` - ❌ Missing
- `mod(vec4, scalar)` - ❌ Missing
- `vec4(vec3.x, vec3.y, vec3.z, scalar)` constructor - ❌ Missing
- `dot(vec3, vec3)` - ✅ Available
- `dot(vec4, vec4)` - ✅ Available
- `vec4 * scalar` - ✅ Available
- `vec4 + vec4` - ✅ Available
- `vec4 - vec4` - ✅ Available
- `sin(vec4)`, `cos(vec4)` - Need component-wise wrappers
- `sqrt(vec4)` - Need component-wise wrapper

**Suggested Answer:** To match GLSL exactly, we need:

1. Vector math operations: `floor()`, `fract()`, `step()`, `min()`, `max()`
2. Extended swizzles: `.xyx`, `.yzz`, `.xyx`, etc.
3. Comparison operations: `greaterThan()`, `any()`
4. Vec4 operations: `mod()`, `floor()`, `fract()`, component-wise trig/sqrt
5. Constructor helpers for Vec4 from Vec3 + scalar
6. Component-wise wrappers for trig functions (sin, cos, sqrt) on vectors

## Directory Structure

**Helper code organization:**

- `/glsl/q32/types/` - Wrapper types (Vec2Q32, Vec3Q32, Vec4Q32, Q32, matrices)
- `/glsl/q32/fns/` - Standalone functions (currently empty, will contain helper functions)

**Implementation approach:**

- Standalone functions go in `/glsl/q32/fns/` (e.g., `floor()`, `fract()`, `step()`)
- Methods on wrapper types go directly on the types (e.g., `Vec3Q32::floor()`, `Vec3Q32::fract()`)

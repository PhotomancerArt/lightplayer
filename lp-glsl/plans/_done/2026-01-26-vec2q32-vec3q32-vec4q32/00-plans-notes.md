# Plan: Implement vec2q32, vec3q32, vec4q32

## Questions

### Q1: Should vector types use Q32 wrapper or raw i32?

**Context:** The reference implementation uses a `Fixed` wrapper type (similar to our `Q32`), but the q32 builtin functions operate on raw `i32` values. The `Q32` type in `util/q32.rs` is a newtype wrapper (`pub struct Q32(pub i32)`), which is zero-cost - Rust optimizes it away completely.

**Answer:** Use `Q32` wrapper type for the public API (like the reference), but internally call the `__lp_q32_*` builtin functions. This provides:
- Clean API similar to reference implementation
- Type safety
- Easy to use in Rust code ported from GLSL
- Zero runtime overhead (newtype wrapper is optimized away)

### Q2: Should we implement GLSL-style swizzle methods?

**Context:** The reference implementation includes extensive swizzle methods (`.xy()`, `.xyz()`, `.rg()`, `.st()`, etc.) which are very useful for porting GLSL code.

**Answer:** Yes, implement swizzle methods similar to the reference. This is a key feature for porting GLSL code to Rust. Include:
- Component accessors: `.x()`, `.y()`, `.z()`, `.w()`
- Color accessors: `.r()`, `.g()`, `.b()`, `.a()`
- Texture accessors: `.s()`, `.t()`, `.p()`, `.q()`
- 2-component swizzles: `.xy()`, `.yx()`, `.rg()`, etc.
- 3-component swizzles: `.xyz()`, `.xzy()`, etc. (for vec3/vec4)
- 4-component swizzles: `.xyzw()`, `.rgba()`, etc. (for vec4)

### Q3: Which operations should use builtin functions vs direct arithmetic?

**Context:** We have builtin functions for `add`, `sub`, `mul`, `div`, `sqrt`, etc. that handle overflow/saturation. However, Q32's operators are fast (no saturation) - they're designed for performance. The reference implementation uses direct operator overloading.

**Answer:** Use Q32's operators directly for fast performance. These are internal utilities that should be fast over safe. Use Q32's operators:
- `Vec2 + Vec2` uses `Q32::add` (direct addition) for each component
- `Vec2 * Q32` uses `Q32::mul` (fast multiply) for each component
- `Vec2 / Q32` uses `Q32::div` (fast divide) for each component
- `length()` uses `__lp_q32_sqrt` for sqrt (since we need a sqrt function, but the vector operations themselves use Q32 operators)

If saturation is needed, users can use the builtin functions directly.

### Q4: Should we implement all methods from the reference or a subset?

**Context:** The reference implementation has many methods including:
- Basic operations: `new()`, `from_f32()`, `from_i32()`, `zero()`, `one()`
- Math operations: `dot()`, `cross()`, `length()`, `length_squared()`, `distance()`, `normalize()`
- Component operations: `mul_comp()`, `div_comp()`, `clamp()`
- Swizzles: extensive swizzle methods
- Operator overloads: `Add`, `Sub`, `Mul<Fixed>`, `Div<Fixed>`, `Neg`

**Answer:** Implement the full set from the reference, as the goal is to make porting GLSL code easy. This includes all the methods listed above.

### Q5: Should vec2/vec3/vec4 be separate files or one file?

**Context:** The reference implementation has separate files: `vec2.rs`, `vec3.rs`, `vec4.rs`. The current project structure has separate files for each q32 builtin function.

**Answer:** Use separate files with explicit naming: `vec2_q32.rs`, `vec3_q32.rs`, `vec4_q32.rs`. This keeps the code organized, makes it easier to navigate, and avoids naming conflicts. Update `util/mod.rs` to export them.

### Q6: Should we include tests similar to the reference?

**Context:** The reference implementation has comprehensive tests. The q32 builtin functions also have tests.

**Answer:** Yes, include comprehensive tests similar to the reference implementation. Use the `test_helpers` module for conversion utilities. Test:
- Construction methods
- Basic arithmetic operations
- Dot product, cross product (vec3 only)
- Length and normalization
- Distance calculations
- Component-wise operations
- Swizzle methods
- Edge cases (zero vectors, normalization of zero vectors, etc.)

### Q7: Should vec3 have a cross product that returns Vec3?

**Context:** In GLSL, `cross(vec3, vec3)` returns `vec3`. The reference implementation follows this.

**Answer:** Yes, match GLSL behavior: `Vec3::cross()` should return `Vec3`. For `Vec2`, `cross()` returns a scalar (the z-component of the 3D cross product), which matches GLSL behavior.

### Q8: Should we implement reflect() method for vec3/vec4?

**Context:** The reference implementation has `reflect()` for `Vec3`. This is useful for lighting calculations.

**Answer:** Yes, implement `reflect()` for `Vec3` (and potentially `Vec4` if it makes sense). This is a common operation in graphics code.

### Q9: How should we handle no_std compatibility?

**Context:** The crate uses `#![cfg_attr(not(feature = "std"), no_std)]` and the q32 builtin functions are `no_std` compatible.

**Answer:** Ensure all vector types are `no_std` compatible. Use `core::ops` instead of `std::ops`. Tests can use `extern crate std` when needed (like the q32 builtin tests do).

### Q10: Should we export the types at the crate root or only from util module?

**Context:** The reference implementation re-exports types at the module level. The current project structure exports q32 builtins from `builtins/q32/mod.rs`.

**Answer:** Export from `util/mod.rs` only. If needed later, they can be re-exported at the crate root, but for now keep them in the util module to match the project structure.

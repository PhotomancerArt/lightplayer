# Plan: Update Old LPFX Functions to Use Overloads

## Questions

### Q1: What should the new function names and signatures be?

**Context:** Currently, the old functions use numbered suffixes:
- `lpfx_hash1`, `lpfx_hash2`, `lpfx_hash3` → should become `lpfx_hash` with overloads
- `lpfx_snoise1`, `lpfx_snoise2`, `lpfx_snoise3` → should become `lpfx_snoise` with overloads
- `lpfx_worley2`, `lpfx_worley3` → should become `lpfx_worley` with overloads
- `lpfx_worley2_value`, `lpfx_worley3_value` → should become `lpfx_worley_value` with overloads

**Answer:** 
- `lpfx_hash(uint x, uint seed)` → `lpfx_hash(uint x, uint seed)` (1D)
- `lpfx_hash(uvec2 xy, uint seed)` → `lpfx_hash(uvec2 xy, uint seed)` (2D)
- `lpfx_hash(uvec3 xyz, uint seed)` → `lpfx_hash(uvec3 xyz, uint seed)` (3D)
- `lpfx_snoise(float x, uint seed)` → `lpfx_snoise(float x, uint seed)` (1D)
- `lpfx_snoise(vec2 p, uint seed)` → `lpfx_snoise(vec2 p, uint seed)` (2D)
- `lpfx_snoise(vec3 p, uint seed)` → `lpfx_snoise(vec3 p, uint seed)` (3D)
- `lpfx_worley(vec2 p, uint seed)` → `lpfx_worley(vec2 p, uint seed)` (2D)
- `lpfx_worley(vec3 p, uint seed)` → `lpfx_worley(vec3 p, uint seed)` (3D)
- `lpfx_worley_value(vec2 p, uint seed)` → `lpfx_worley_value(vec2 p, uint seed)` (2D)
- `lpfx_worley_value(vec3 p, uint seed)` → `lpfx_worley_value(vec3 p, uint seed)` (3D)

Hash functions should use `uvec2`/`uvec3` types instead of multiple `uint` parameters. This matches the pattern used by newer functions like `lpfx_hsv2rgb` which use `vec3`/`vec4`.

### Q2: Should we maintain backward compatibility with the old names?

**Context:** There are test files and potentially user code that uses the old names (`lpfx_hash1`, `lpfx_snoise1`, etc.). We could either:
- Option A: Remove the old names entirely (breaking change)
- Option B: Keep the old names as aliases that map to the new overloaded names
- Option C: Keep the old names temporarily with deprecation warnings

**Answer:** Option A - Remove the old names entirely. This is cleaner and matches the new library style. Test files will need to be updated to use the new names.

### Q3: What about the internal function names and structure?

**Context:** The internal function names (the actual Rust functions) currently have numbered suffixes or dimension indicators. These are used by the builtin registry and codegen.

**Answer:** 
- Keep the internal function names as-is (`__lpfx_hash_1`, `__lpfx_hash_2`, `__lpfx_hash_3`, etc.). These are implementation details and don't need to change.
- Follow the pattern from newer functions like `hsv2rgb`:
  - Public Rust function `lpfx_hash` that takes helper types (for hash: individual `u32` params since no UVec types exist yet; for snoise/worley: `Vec2Q32`/`Vec3Q32`)
  - Extern C functions `__lpfx_hash_*` that use expanded types (individual `u32`/`i32` parameters)
  - The extern C functions call the public Rust functions with conversions

### Q4: How should we handle the macro annotations?

**Context:** Currently, functions use annotations like:
- `#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash1(uint x, uint seed)")]`
- `#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise1(float x, uint seed)")]`

**Answer:** Change the GLSL signature strings to use the unified names with vector types:
- `#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uint x, uint seed)")]` (1D)
- `#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uvec2 xy, uint seed)")]` (2D)
- `#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uvec3 xyz, uint seed)")]` (3D)
- `#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise(float x, uint seed)")]` (1D)
- `#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise(vec2 p, uint seed)")]` (2D)
- `#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise(vec3 p, uint seed)")]` (3D)
- etc.

The codegen tool will generate multiple entries with the same name but different signatures, which the overload resolution system will handle.

### Q5: What about the hash function's structure?

**Context:** Looking at `hash.rs`, the current structure has:
- Three separate extern C functions: `__lpfx_hash_1`, `__lpfx_hash_2`, `__lpfx_hash_3`
- They all call a shared `hash_impl` function
- The GLSL signatures use numbered names: `lpfx_hash1`, `lpfx_hash2`, `lpfx_hash3`

**Answer:** 
- Add public Rust functions `lpfx_hash` (taking individual `u32` params since no UVec helper types exist)
- Keep the extern C functions as-is but update their GLSL signatures to use `lpfx_hash` with `uvec2`/`uvec3` types
- The extern C functions will extract components from the expanded parameters and call the public Rust functions
- For 2D/3D hash, the extern C functions receive expanded `u32` parameters (from `uvec2`/`uvec3`), so they'll take `x, y, seed` or `x, y, z, seed` and call the public function

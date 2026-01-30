# Plan: Adapt Lygia FBM Noise to LPFX Function

## Overview

Adapt the Fractal Brownian Motion (FBM) noise function from Lygia to work as an LPFX builtin function. The FBM function combines multiple octaves of noise to create fractal patterns.

## Source Material

Source: `/Users/yona/dev/photomancer/oss/lygia/generative/fbm.glsl`

The GLSL implementation provides:

- 2D FBM: `fbm(vec2 st)` - uses 2D noise
- 3D FBM: `fbm(vec3 pos)` - uses 3D noise
- 3D Tilable FBM: `fbm(vec3 p, float tileLength)` - uses tilable 3D noise

## Questions

### Q1: Which FBM variants should we implement?

**Context:**
The GLSL source has three variants:

1. `fbm(vec2 st)` - 2D version using `FBM_NOISE2_FNC` (defaults to snoise)
2. `fbm(vec3 pos)` - 3D version using `FBM_NOISE3_FNC` (defaults to snoise)
3. `fbm(vec3 p, float tileLength)` - 3D tilable version using `FBM_NOISE3_TILABLE_FNC` (defaults to gnoise)

**Suggested Answer:**
Implement all three variants:

- `lpfx_fbm(vec2 p, int octaves, uint seed)` - 2D FBM
- `lpfx_fbm(vec3 p, int octaves, uint seed)` - 3D FBM
- `lpfx_fbm(vec3 p, float tileLength, int octaves, uint seed)` - 3D tilable FBM

This matches the pattern used by other noise functions (snoise, worley) which have multiple overloads.

### Q2: Should we implement gnoise for the tilable variant?

**Context:**
The tilable FBM variant uses `gnoise` (gradient noise) which we don't currently have. The GLSL code uses `FBM_NOISE3_TILABLE_FNC` which defaults to `gnoise(UV, TILE)`.

**Options:**

1. Implement gnoise first as a separate function, then use it in fbm
2. Use psrdnoise for the tilable variant (it already supports tiling)
3. Skip the tilable variant for now

**Answer:**
Implement gnoise and all its dependencies. This includes:

- `random()` functions (1D, 2D, 3D) - returns [0, 1] using sin-based hash
- `srandom()` functions (1D, 2D, 3D) - returns [-1, 1] (signed random)
- `srandom3()` with tileLength support - returns vec3 in [-1, 1] range
- `cubic()` interpolation function - cubic polynomial smoothing
- `quintic()` interpolation function - quintic polynomial smoothing
- `gnoise()` functions (1D, 2D, 3D, and 3D tilable) - gradient noise

### Q3: What should be configurable parameters vs constants?

**Context:**
The GLSL code uses preprocessor macros for configuration:

- `FBM_OCTAVES` (default 4) - number of octaves
- `FBM_VALUE_INITIAL` (default 0.0) - initial value
- `FBM_SCALE_SCALAR` (default 2.0) - scale multiplier (lacunarity)
- `FBM_AMPLITUDE_INITIAL` (default 0.5) - initial amplitude
- `FBM_AMPLITUDE_SCALAR` (default 0.5) - amplitude multiplier (persistence)

The tilable variant hardcodes:

- `persistence = 0.5`
- `lacunarity = 2.0`

**Answer:**

- **Parameters:** `octaves` (required, as specified by user)
- **Constants:** Use the GLSL defaults for all other values:
  - Initial value: 0.0
  - Scale scalar (lacunarity): 2.0
  - Amplitude initial: 0.5
  - Amplitude scalar (persistence): 0.5

This keeps the API simple while matching the GLSL defaults. We can add more parameters later if needed.

### Q4: Should we create helper functions to match GLSL structure?

**Context:**
The user wants to keep Rust code as close as possible to GLSL code. The GLSL code has a simple loop structure:

```glsl
for (int i = 0; i < FBM_OCTAVES; i++) {
    value += amplitude * FBM_NOISE2_FNC(st);
    st *= FBM_SCALE_SCALAR;
    amplitude *= FBM_AMPLITUDE_SCALAR;
}
```

**Answer:**
Yes, create helper functions that mirror the GLSL structure:

- Keep the loop structure identical
- Use helper functions for noise calls (snoise2, snoise3, gnoise3 for tilable)
- Use constants for the default values
- This will make the code easier to verify against the GLSL source

### Q5: What about the tilable variant's normalization?

**Context:**
The tilable variant accumulates a `normalization` value and divides the result by it:

```glsl
normalization += amplitude;
// ...
return total / normalization;
```

The non-tilable variants don't normalize.

**Answer:**
Keep the normalization in the tilable variant as it appears in the GLSL source. This is likely important for the tilable version to work correctly.

### Q6: Should we support both f32 and q32 implementations?

**Context:**
Other noise functions (snoise, psrdnoise, worley) have both f32 and q32 implementations. The f32 versions typically call the q32 versions with conversion.

**Answer:**
Yes, implement both:

- q32 versions with the actual implementation
- f32 versions that convert to q32, call q32 version, convert back

This matches the pattern used by other noise functions.

## Notes

- The user wants to keep Rust code as close as possible to GLSL code
- We should create helper functions as needed to allow this
- Octaves should be specified as an argument (not a preprocessor macro)
- We don't have preprocessor support, so all configuration must be via function parameters or constants

## Dependencies Available

- ✅ Hash functions: `lpfx_hash`, `lpfx_hash2`, `lpfx_hash3` (but different algorithm than GLSL random)
- ✅ Sin functions: `__lp_q32_sin` available
- ✅ Floor/fract: `Q32::to_i32()` (floor), `Q32::frac()` (fract), `Vec2Q32::floor()`, `Vec2Q32::fract()`, etc.
- ❌ Mix/lerp: Not available, should be implemented as methods on Q32 and vector types: `mix(a, b, t) = a + t * (b - a)`
- ❌ Random functions: Need to implement using sin-based hash (different from our hash)
- ❌ Srandom functions: Need to implement (signed random)
- ❌ Cubic/quintic interpolation: Need to implement
- ❌ Gnoise: Need to implement

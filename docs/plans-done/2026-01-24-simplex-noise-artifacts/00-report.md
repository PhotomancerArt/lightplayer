# Simplex Noise Artifacts Investigation Report

## Summary

The simplex noise implementation exhibits visible artifacts including diagonal lines, sharp
transitions, and jagged edges. Investigation reveals **critical sign errors** in offset calculations
for 2D and 3D simplex noise that cause discontinuities at cell boundaries.

## Critical Bugs Identified

### 1. **Sign Error in 2D Offset Calculation** (CRITICAL)

**Location**: `lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs:98-99`

**Current (WRONG)**:

```rust
let offset2_x = offset1_x - order_x - UNSKEW_FACTOR_2D;
let offset2_y = offset1_y - order_y - UNSKEW_FACTOR_2D;
```

**Should be**:

```rust
let offset2_x = offset1_x - order_x + UNSKEW_FACTOR_2D;
let offset2_y = offset1_y - order_y + UNSKEW_FACTOR_2D;
```

**Reference**: noise-rs `simplex.rs:95`:

```rust
let offset2 = offset1 - order + unskew_factor;
```

**Impact**: This causes incorrect corner offset calculations, leading to discontinuities and
diagonal artifacts.

### 2. **Sign Error in 3D Offset Calculations** (CRITICAL)

**Location**: `lp-glsl-builtins/src/builtins/q32/lpfx_snoise3.rs:163-165`

**Current (WRONG)**:

```rust
let offset2_x = offset1_x - order1_x - UNSKEW_FACTOR_3D;
let offset2_y = offset1_y - order1_y - UNSKEW_FACTOR_3D;
let offset2_z = offset1_z - order1_z - UNSKEW_FACTOR_3D;
```

**Should be**:

```rust
let offset2_x = offset1_x - order1_x + UNSKEW_FACTOR_3D;
let offset2_y = offset1_y - order1_y + UNSKEW_FACTOR_3D;
let offset2_z = offset1_z - order1_z + UNSKEW_FACTOR_3D;
```

**Reference**: noise-rs `simplex.rs:224`:

```rust
let offset2 = offset1 - order1.numcast().unwrap() + unskew_factor;
```

**Impact**: Same as 2D - causes discontinuities in 3D noise.

### 3. **Additional Sign Errors in offset3 and offset4** (CRITICAL)

**2D offset3** (`lpfx_snoise2.rs:102-103`):

- Current (WRONG): `offset3_x = offset1_x - Q32::ONE - (TWO * UNSKEW_FACTOR_2D)`
- Should be: `offset3_x = offset1_x - Q32::ONE + (TWO * UNSKEW_FACTOR_2D)`

**3D offset3** (`lpfx_snoise3.rs:167-169`):

- Current (WRONG): `offset3_x = offset1_x - order2_x - (TWO * UNSKEW_FACTOR_3D)`
- Should be: `offset3_x = offset1_x - order2_x + (TWO * UNSKEW_FACTOR_3D)`

**3D offset4** (`lpfx_snoise3.rs:171-173`):

- Current (WRONG): `offset4_x = offset1_x - Q32::ONE - Q32::ONE - (THREE * UNSKEW_FACTOR_3D)`
- Should be: `offset4_x = offset1_x - Q32::ONE - Q32::ONE + (THREE * UNSKEW_FACTOR_3D)`

**Reference**: noise-rs `simplex.rs`:

- 2D: `offset3 = offset1 - 1.0 + 2.0 * unskew_factor` (line 97)
- 3D: `offset3 = offset1 - order2 + 2.0 * unskew_factor` (line 225)
- 3D: `offset4 = offset1 - 1.0 + 3.0 * unskew_factor` (line 226)

**Impact**: All offsets that involve unskew_factor were subtracting instead of adding, causing
severe discontinuities at all corner boundaries.

## Comparison with References

### noise-rs Implementation

**2D Simplex** (`src/core/simplex.rs`):

- Skew factor: `(sqrt(3) - 1) / 2` ✓ (matches our `SKEW_FACTOR_2D`)
- Unskew factor: `(1 - 1/sqrt(3)) / 2` = `(3 - sqrt(3)) / 6` ✓ (matches our `UNSKEW_FACTOR_2D`)
- Offset calculations: `offset2 = offset1 - order + unskew_factor` ❌ (we subtract instead of add)

**3D Simplex** (`src/core/simplex.rs`):

- Skew factor: `(sqrt(4) - 1) / 3` = `1/3` ✓ (matches our `SKEW_FACTOR_3D`)
- Unskew factor: `(1 - 1/sqrt(4)) / 3` = `(1 - 0.5) / 3` = `1/6` ✓ (matches our `UNSKEW_FACTOR_3D`)
- Offset calculations: `offset2 = offset1 - order1 + unskew_factor` ❌ (we subtract instead of add)

### GLSL Reference Implementation

The user-provided GLSL reference uses:

```glsl
vec4 x12 = x0.xyxy + C.xxzz;
x12.xy -= i1;
```

This translates to: `x12 = x0 + C.xxzz`, then `x12.xy = x12.xy - i1`, which is equivalent to
`x12.xy = x0.xy + C.xx - i1`.

This matches the noise-rs pattern: `offset2 = offset1 - order + unskew_factor`.

## Other Observations

### Gradient Selection

- Our gradient tables (`grad2`, `grad3`) match noise-rs exactly ✓
- Hash function usage appears correct ✓

### Surflet Calculation

- Falloff function `(2.0 * t^2 + t^4)` matches noise-rs ✓
- Distance calculation `t = 1.0 - dist^2 * 2.0` matches noise-rs ✓

### Skew/Unskew Factors

- All factors match noise-rs calculations ✓
- Fixed-point conversions appear correct ✓

## Root Cause Analysis

The artifacts (diagonal lines, sharp transitions) are caused by **incorrect corner offset
calculations**. When the unskew factor is subtracted instead of added, the offsets for corners 2 and
3 (in 2D) or corners 2, 3, and 4 (in 3D) are positioned incorrectly relative to the simplex cell
boundaries. This creates discontinuities where contributions from adjacent cells don't blend
smoothly.

## Testing Recommendations

1. **Add unit tests with fixed hash values**:
    - Use `#[cfg(test_hash_fixed)]` feature to replace hash function with deterministic values
    - Test specific known points against reference implementations
    - Verify continuity across cell boundaries

2. **Visual regression tests**:
    - Generate noise images and compare against reference implementations
    - Check for diagonal artifacts, sharp transitions, and smooth gradients

3. **Boundary tests**:
    - Test points exactly on cell boundaries
    - Test points near boundaries (epsilon away)
    - Verify smooth transitions

4. **Reference comparison tests**:
    - Compare outputs with noise-rs at specific test points
    - Account for hash function differences but verify algorithm correctness

## Next Steps

1. Fix the sign errors in offset calculations for 2D and 3D
2. Add comprehensive tests with fixed hash values
3. Verify visual output matches reference implementations
4. Consider adding octave/fractal noise support (user mentioned interest)

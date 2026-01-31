# Fix Offset Calculation Sign Errors

## Description

Fix the critical sign errors in 2D and 3D simplex noise offset calculations that cause diagonal
artifacts and discontinuities.

## Changes Required

### 1. Fix 2D Simplex Offset Calculation

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs`

**Line 98-99**: Change from:

```rust
let offset2_x = offset1_x - order_x - UNSKEW_FACTOR_2D;
let offset2_y = offset1_y - order_y - UNSKEW_FACTOR_2D;
```

To:

```rust
let offset2_x = offset1_x - order_x + UNSKEW_FACTOR_2D;
let offset2_y = offset1_y - order_y + UNSKEW_FACTOR_2D;
```

### 2. Fix 3D Simplex Offset Calculation

**File**: `lp-glsl/lp-glsl-builtins/src/builtins/q32/lpfx_snoise3.rs`

**Line 163-165**: Change from:

```rust
let offset2_x = offset1_x - order1_x - UNSKEW_FACTOR_3D;
let offset2_y = offset1_y - order1_y - UNSKEW_FACTOR_3D;
let offset2_z = offset1_z - order1_z - UNSKEW_FACTOR_3D;
```

To:

```rust
let offset2_x = offset1_x - order1_x + UNSKEW_FACTOR_3D;
let offset2_y = offset1_y - order1_y + UNSKEW_FACTOR_3D;
let offset2_z = offset1_z - order1_z + UNSKEW_FACTOR_3D;
```

## Verification

1. Run existing tests to ensure no regressions
2. Visual inspection: Generate noise images and verify artifacts are gone
3. Compare outputs with noise-rs reference at specific test points

## Success Criteria

- All existing tests pass
- Visual artifacts (diagonal lines, sharp transitions) are eliminated
- Noise output appears smooth and continuous across cell boundaries

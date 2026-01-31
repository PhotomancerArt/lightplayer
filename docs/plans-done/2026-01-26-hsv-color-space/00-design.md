# Design: HSV Color Space Functions

## Overview

Port HSV/HSB color space conversion functions from lygia to lp-glsl-builtins, using Q32 fixed-point
arithmetic and the new q32 vector helpers (Vec3Q32, Vec4Q32). This enables easy porting of GLSL
color manipulation code to Rust.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/builtins/lpfx/
├── color/
│   └── space/
│       ├── mod.rs                    # NEW: Module declaration for color/space
│       ├── hue2rgb_q32.rs            # NEW: Hue to RGB conversion (q32)
│       ├── hsv2rgb_q32.rs            # NEW: HSV to RGB conversion (q32)
│       └── rgb2hsv_q32.rs            # NEW: RGB to HSV conversion (q32)
├── math/
│   ├── mod.rs                        # NEW: Module declaration for math
│   └── saturate_q32.rs               # NEW: Saturate/clamp function (q32)
└── mod.rs                            # UPDATE: Export color and math modules
```

## Types Summary

### Math Module (`math/saturate_q32.rs`)

```
lpfx_saturate_q32(value: Q32) -> Q32
  # NEW: Clamp value between 0 and 1 (Q32 fixed-point)
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_saturate_q32(i32) -> i32

lpfx_saturate_vec3_q32(v: Vec3Q32) -> Vec3Q32
  # NEW: Saturate each component of vec3
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_saturate_vec3_q32(i32, i32, i32) -> (i32, i32, i32)

lpfx_saturate_vec4_q32(v: Vec4Q32) -> Vec4Q32
  # NEW: Saturate each component of vec4
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_saturate_vec4_q32(i32, i32, i32, i32) -> (i32, i32, i32, i32)
```

### Color/Space Module (`color/space/hue2rgb_q32.rs`)

```
lpfx_hue2rgb_q32(hue: Q32) -> Vec3Q32
  # NEW: Convert hue value (0-1) to RGB vec3
  # Public Rust function - can be inlined, called by hsv2rgb
  # Extern C wrapper: __lpfx_hue2rgb_q32(i32) -> (i32, i32, i32)
  # Algorithm: Uses abs() and arithmetic to compute RGB from hue
  # Uses: lpfx_saturate_vec3_q32
```

### Color/Space Module (`color/space/hsv2rgb_q32.rs`)

```
lpfx_hsv2rgb_q32(hsv: Vec3Q32) -> Vec3Q32
  # NEW: Convert HSV to RGB
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_hsv2rgb_q32(i32, i32, i32) -> (i32, i32, i32)
  # Uses: lpfx_hue2rgb_q32, lpfx_saturate_vec3_q32

lpfx_hsv2rgb_vec4_q32(hsv: Vec4Q32) -> Vec4Q32
  # NEW: Convert HSV to RGB (with alpha channel preserved)
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_hsv2rgb_vec4_q32(i32, i32, i32, i32) -> (i32, i32, i32, i32)
  # Uses: lpfx_hsv2rgb_q32
```

### Color/Space Module (`color/space/rgb2hsv_q32.rs`)

```
lpfx_rgb2hsv_q32(rgb: Vec3Q32) -> Vec3Q32
  # NEW: Convert RGB to HSV
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_rgb2hsv_q32(i32, i32, i32) -> (i32, i32, i32)
  # Uses: epsilon constant to avoid division by zero

lpfx_rgb2hsv_vec4_q32(rgb: Vec4Q32) -> Vec4Q32
  # NEW: Convert RGB to HSV (with alpha channel preserved)
  # Public Rust function - can be inlined, called by other lpfx functions
  # Extern C wrapper: __lpfx_rgb2hsv_vec4_q32(i32, i32, i32, i32) -> (i32, i32, i32, i32)
  # Uses: lpfx_rgb2hsv_q32
```

## Implementation Details

### Epsilon Handling

The `rgb2hsv` function uses an epsilon value to avoid division by zero. For Q32:

- Use minimum representable Q32 value or a very small but representable Q32 value
- Ensure epsilon is actually useful for avoiding division by zero
- Test cases must cover epsilon scenarios (colors with very small or zero differences between RGB
  components)

### Function Pattern

Each function follows this two-layer pattern:

1. **`lpfx_*`** - Public Rust function with nice types (Q32, Vec3Q32, Vec4Q32)
    - Contains the actual implementation
    - Can be inlined when called from other Rust code
    - Allows ergonomic calls between lpfx functions (e.g., `hsv2rgb` can call `hue2rgb` and
      `saturate` with nice types)

2. **`__lpfx_*`** - Extern C wrapper with expanded types (i32, flattened vectors)
    - Wraps the `lpfx_*` function for compiler/GLSL calls
    - Takes expanded types: Q32 becomes i32, Vec3Q32 becomes three i32 parameters
    - Has `#[lpfx_impl_macro::lpfx_impl]` annotation for auto-registration
    - Has `#[unsafe(no_mangle)]` and `pub extern "C"` attributes

Example:

```rust
// Public Rust API - can be inlined
#[inline(always)]
pub fn lpfx_saturate_q32(value: Q32) -> Q32 {
    // Actual implementation
    value.max(Q32::ZERO).min(Q32::ONE)
}

// Extern C wrapper for compiler
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_saturate(float x)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_saturate_q32(value: i32) -> i32 {
    lpfx_saturate_q32(Q32::from_fixed(value)).to_fixed()
}
```

**Note:** This is a new pattern being established. Existing functions (noise, hash) should also be
refactored to follow this pattern in the future, but that's out of scope for this plan.

### Dependencies

- `lpfx_saturate_q32` is a simple utility (clamp to [0, 1])
- `lpfx_hue2rgb_q32` is a color-specific helper, uses `lpfx_saturate_vec3_q32`
- `lpfx_hsv2rgb_q32` depends on `lpfx_hue2rgb_q32` and `lpfx_saturate_vec3_q32`
- `lpfx_rgb2hsv_q32` is standalone (uses epsilon constant)

All functions call each other using the `lpfx_*` names with nice types, allowing inlining and
ergonomic Rust code.

## Testing Requirements

- Basic conversion tests (known RGB <-> HSV pairs)
- Round-trip tests (RGB -> HSV -> RGB should be approximately equal)
- Edge cases: pure colors (red, green, blue), grayscale, black, white
- Epsilon case: colors with very small or zero differences between RGB components
- Vec3 and Vec4 variants tested
- Range validation (HSV values should be in [0, 1] range)

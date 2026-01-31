# Plan: Port HSV Color Space Functions from Lygia

## Questions

### Q1: Which HSV functions should we implement?

**Context:** Lygia has three HSV-related functions:

- `rgb2hsv.glsl` - Converts RGB to HSV/HSB color space
- `hsv2rgb.glsl` - Converts HSV to RGB (depends on `hue2rgb` and `saturate`)
- `hsv2ryb.glsl` - Converts HSV to RYB color space (depends on `hsv2rgb`, `rgb2ryb`, `ryb2rgb`, and
  `saturate`)

**Answer:** Implement the two core HSV functions: `rgb2hsv` and `hsv2rgb`. Skip `hsv2ryb` for now.

### Q2: What dependencies need to be implemented?

**Context:** The HSV functions have dependencies:

- `hsv2rgb` needs:
    - `hue2rgb` - Converts hue value to RGB vec3
    - `saturate` - Clamps value between 0 and 1 (simple clamp)
- `rgb2hsv` is standalone (uses epsilon constant)

**Answer:**

- Implement both `saturate` and `hue2rgb` following lygia's structure:
    - `saturate` in `lpfx/math/` (following lygia's structure)
    - `hue2rgb` in `lpfx/color/space/` (color-specific)
- For small helper functions, separate the `__lp` extern C version (for remote GLSL calls) from the
  actual implementation (which can be inlined):
    - Public Rust function: does the actual work, can be inlined
    - `extern "C"` function: wraps the implementation for remote calls

### Q3: Should we implement both f32 and q32 versions?

**Context:** Looking at existing lpfx functions (like `snoise`), they have both `_f32` and `_q32`
versions. The f32 versions are typically stubs that convert to q32, call the q32 version, and
convert back.

**Answer:** Focus on q32 versions for now since float support isn't available yet. We can add f32
stubs later following the same pattern as other lpfx functions if needed.

### Q4: What directory structure should we use?

**Context:** The user wants to keep the same directory structure that lygia uses relative to
`lp-glsl/lp-glsl-builtins/src/builtins/lpfx`. Lygia has:

- `color/space/hsv2rgb.glsl`
- `color/space/rgb2hsv.glsl`
- `color/space/hue2rgb.glsl`
- `math/saturate.glsl`

**Answer:** Create the following structure:

- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/` for HSV functions and `hue2rgb`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/math/` for `saturate`

### Q5: Should saturate be in a math subdirectory or color subdirectory?

**Context:** In lygia, `saturate` is in `math/saturate.glsl`, but it's used by color functions. We
could either:

- Create `lpfx/math/` for math utilities
- Put it in `lpfx/color/space/` since it's primarily used by color functions
- Put it in `lpfx/color/` as a general color utility

**Suggested Answer:** Create `lpfx/math/` subdirectory for math utilities like `saturate`, following
lygia's structure. This keeps the organization clean and allows for future math utilities.

### Q6: How should we handle the epsilon constant in rgb2hsv?

**Context:** The `rgb2hsv` function uses `HCV_EPSILON` (defined as `1e-10`) to avoid division by
zero. This is a very small float value that may not be accurately representable in Q32 fixed-point
format (16.16).

**Answer:** Use the minimum representable Q32 value or a very small but representable Q32 value
instead of trying to convert `1e-10` directly. This ensures the epsilon is actually useful for
avoiding division by zero in the Q32 implementation.

**Note:** Ensure tests cover the epsilon case - test with colors that would cause division by zero
without epsilon (e.g., very small or zero differences between RGB components).

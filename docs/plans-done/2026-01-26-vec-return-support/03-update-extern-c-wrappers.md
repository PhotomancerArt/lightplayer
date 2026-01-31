# Phase 3: Update Extern C Wrappers to Use StructReturn

## Description

Update all extern C wrappers that return vectors to use StructReturn. The wrappers will take a
pointer parameter and write all components to memory.

## Changes

### Files to Update

- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/hue2rgb_q32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/hue2rgb_f32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/hsv2rgb_q32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/hsv2rgb_f32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/rgb2hsv_q32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/color/space/rgb2hsv_f32.rs`
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/math/saturate_q32.rs` (vec3/vec4 variants)
- `lp-glsl/lp-glsl-builtins/src/builtins/lpfx/math/saturate_f32.rs` (vec3/vec4 variants)

### Pattern for Each Wrapper

Change from:

```rust
pub extern "C" fn __lpfx_hue2rgb_q32(hue: i32) -> i32 {
    let result = lpfx_hue2rgb_q32(Q32::from_fixed(hue));
    result.x.to_fixed()
}
```

To:

```rust
pub extern "C" fn __lpfx_hue2rgb_q32(result_ptr: *mut i32, hue: i32) {
    let result = lpfx_hue2rgb_q32(Q32::from_fixed(hue));
    unsafe {
        *result_ptr.offset(0) = result.x.to_fixed();
        *result_ptr.offset(1) = result.y.to_fixed();
        *result_ptr.offset(2) = result.z.to_fixed();
    }
}
```

## Success Criteria

- All vector-returning extern C wrappers take StructReturn pointer as first parameter
- All components are written to memory at correct offsets
- Scalar-returning wrappers unchanged
- Code compiles without errors

## Implementation Notes

- Use `unsafe` blocks for pointer operations
- Offset calculation: `result_ptr.offset(0)` for x, `offset(1)` for y, etc.
- For Vec4: also write w component at `offset(3)`
- Keep return type as `()` (void) for StructReturn functions

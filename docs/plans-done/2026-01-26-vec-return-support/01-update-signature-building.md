# Phase 1: Update Signature Building for Vector Returns

## Description

Update `build_call_signature()` in `lpfx_sig.rs` to handle vector return types (Vec2, Vec3, Vec4) using StructReturn. This matches how user functions handle vector returns.

## Changes

### `lp-glsl/crates/lp-glsl-compiler/src/frontend/semantic/lpfx/lpfx_sig.rs`

- Update `build_call_signature()` to:
  - Check if return type is a vector (Vec2, Vec3, Vec4)
  - If vector: add StructReturn parameter FIRST, clear returns
  - Get pointer type from ISA
  - Calculate buffer size based on component count
- Add helper function to get pointer type
- Support Vec4 in addition to Vec2/Vec3

## Success Criteria

- `build_call_signature()` handles Vec2, Vec3, Vec4 return types without panicking
- StructReturn parameter is added FIRST (before regular params)
- Returns are cleared for StructReturn functions
- Scalar return types continue to work as before

## Implementation Notes

- Use `ArgumentPurpose::StructReturn` for the parameter
- Buffer size: component_count × 4 bytes (for f32/i32)
- Pointer type: get from ISA using `isa.pointer_type()`

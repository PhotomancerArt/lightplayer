# Design: Add Vector Return Support for LPFX Functions

## Overview

Add support for vector return types (Vec2, Vec3, Vec4) in LPFX functions. Currently, LPFX functions only support scalar returns, causing a panic when encountering vector return types. This design uses StructReturn (pointer parameter) for vector returns, matching how user functions handle vector returns.

## File Structure

```
lp-glsl/crates/lp-glsl-compiler/src/frontend/
├── semantic/lpfx/
│   └── lpfx_sig.rs                    # UPDATE: Add StructReturn support for vector returns
└── codegen/
    └── lpfx_fns.rs                     # UPDATE: Handle StructReturn in LPFX calls

lp-glsl/crates/lp-builtins/src/builtins/lpfx/
├── color/space/
│   ├── hue2rgb_q32.rs                  # UPDATE: Add StructReturn parameter to extern C wrapper
│   ├── hue2rgb_f32.rs                  # UPDATE: Add StructReturn parameter to extern C wrapper
│   ├── hsv2rgb_q32.rs                  # UPDATE: Add StructReturn parameter to extern C wrappers
│   ├── hsv2rgb_f32.rs                  # UPDATE: Add StructReturn parameter to extern C wrappers
│   ├── rgb2hsv_q32.rs                  # UPDATE: Add StructReturn parameter to extern C wrappers
│   └── rgb2hsv_f32.rs                  # UPDATE: Add StructReturn parameter to extern C wrappers
└── math/
    ├── saturate_q32.rs                  # UPDATE: Add StructReturn parameter to vec3/vec4 extern C wrappers
    └── saturate_f32.rs                  # UPDATE: Add StructReturn parameter to vec3/vec4 extern C wrappers
```

## Types and Functions

### `lpfx_sig.rs`

**`build_call_signature()`** - UPDATE
- Add StructReturn parameter for vector return types (Vec2, Vec3, Vec4)
- Insert StructReturn parameter FIRST (before regular params)
- Clear returns (StructReturn functions return void)
- Support Vec4 in addition to Vec2/Vec3

**Helper functions** - NEW
- `get_pointer_type()` - Get pointer type for StructReturn parameter
- `calculate_struct_return_size()` - Calculate buffer size for vector return

### `lpfx_fns.rs`

**`emit_lp_lib_fn_call()`** - UPDATE
- Check if function uses StructReturn
- Allocate stack slot for return buffer
- Pass StructReturn pointer as first argument
- Load return values from buffer after call
- Handle both Decimal and NonDecimal implementations

**`get_lpfx_testcase_call()`** - UPDATE
- No changes needed (signature building handles StructReturn)

### Extern C Wrappers (all vector-returning functions)

**`__lpfx_*_q32()` / `__lpfx_*_f32()`** - UPDATE
- Add `*mut i32` / `*mut f32` parameter for StructReturn (first parameter)
- Write all vector components to memory at offsets
- Return void (or keep current return for compatibility during transition)

## Implementation Details

### StructReturn Pattern

1. **Signature Building**: For vector return types, add StructReturn parameter FIRST:
   ```rust
   sig.params.insert(0, AbiParam::special(pointer_type, ArgumentPurpose::StructReturn));
   sig.returns.clear(); // StructReturn functions return void
   ```

2. **Call Site**: 
   - Allocate stack slot for return buffer
   - Pass buffer pointer as first argument
   - Call function
   - Load values from buffer at offsets (4 bytes per f32/i32)

3. **Extern C Wrappers**:
   - Take pointer parameter as first argument
   - Write components to memory: `*ptr.offset(0) = x; *ptr.offset(1) = y; ...`
   - Return void

### Buffer Size Calculation

- Vec2: 2 components × 4 bytes = 8 bytes
- Vec3: 3 components × 4 bytes = 12 bytes  
- Vec4: 4 components × 4 bytes = 16 bytes

### Offset Calculation

- Component 0: offset 0
- Component 1: offset 4
- Component 2: offset 8
- Component 3: offset 12

## Success Criteria

1. `build_call_signature()` handles Vec2, Vec3, Vec4 return types without panicking
2. `emit_lp_lib_fn_call()` correctly handles StructReturn for vector returns
3. Extern C wrappers write all components to StructReturn buffer
4. GLSL filetests for HSV functions pass
5. Existing scalar return functions continue to work
6. Both Decimal and NonDecimal implementations support vector returns

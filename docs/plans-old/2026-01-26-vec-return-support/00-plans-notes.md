# Plan: Add Vector Return Support for LPFX Functions

## Questions

### Q1: Should we use multiple return values or StructReturn for vector returns?

**Context:** Currently, LPFX functions only support scalar returns (Float, Int, UInt). Vector returns (Vec2, Vec3, Vec4) are not supported. We need to decide between:
- Multiple return values: Function returns multiple scalar values (e.g., vec3 returns 3 f32/i32 values)
- StructReturn: Function takes a pointer parameter and writes the vector components to memory

**Answer:** Use StructReturn for consistency with extern C wrappers and to match how user functions handle vector returns. This simplifies the implementation and ensures compatibility.

### Q2: How should we handle Vec4 returns?

**Context:** Vec4 has 4 components. With StructReturn, all vector types (Vec2, Vec3, Vec4) will be handled the same way.

**Answer:** Vec4 will use StructReturn just like Vec2 and Vec3. No special handling needed.

### Q3: Should we update both Decimal and NonDecimal implementations?

**Context:** LPFX functions can be Decimal (f32/q32 variants) or NonDecimal (like hash functions). Currently only Decimal functions need vector returns (HSV functions), but we should support it for both.

**Answer:** Yes, update both. The signature building and return value handling should work the same way regardless of whether it's Decimal or NonDecimal.

### Q4: How should we handle the extern C wrappers for vector returns?

**Context:** The extern C wrappers (like `__lpfx_hue2rgb_q32`) currently return only `result.x.to_fixed()` (the first component). For proper vector returns, we need to return all components. However, `extern "C"` functions can't return multiple values directly in C.

**Answer:** Use StructReturn (pointer parameter) for vector returns in the extern C wrappers. This is the standard C way to return structs/vectors. The wrapper will take a pointer parameter and write all components to memory. The compiler will handle allocating the buffer and loading the values.

### Q5: Should the compiler use StructReturn or multiple return values?

**Context:** The compiler builds signatures based on GLSL function signatures. For vector returns, we can either:
1. Use StructReturn (pointer parameter) - matches extern C wrappers
2. Use multiple return values - more efficient but requires different calling convention

**Answer:** Use StructReturn for consistency with extern C wrappers and to match how user functions handle vector returns. This simplifies the implementation and ensures compatibility.

# Plan: Implement Vec2Q32, Vec3Q32, Vec4Q32

## Overview

Implement vector types for Q32 fixed-point arithmetic to enable easy porting of GLSL code to Rust. These types provide a clean, ergonomic API similar to GLSL vectors while using fast Q32 fixed-point arithmetic.

## Phases

1. Implement Vec2Q32 with all operations and swizzles
2. Implement Vec3Q32 with all operations and swizzles
3. Implement Vec4Q32 with all operations and swizzles
4. Add comprehensive tests for all vector types
5. Cleanup and finalization

## Success Criteria

- All three vector types (Vec2Q32, Vec3Q32, Vec4Q32) are implemented
- All methods from the reference implementation are included
- GLSL-style swizzle methods are implemented
- Comprehensive tests pass
- Code is `no_std` compatible
- Code follows project conventions (formatting, organization)
- All warnings are fixed
- Code compiles without errors

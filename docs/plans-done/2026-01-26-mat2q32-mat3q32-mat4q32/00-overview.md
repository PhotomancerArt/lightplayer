# Plan: Implement Mat2Q32, Mat3Q32, Mat4Q32

## Overview

Implement matrix types for Q32 fixed-point arithmetic to enable easy porting of GLSL code to Rust. These types provide a clean, ergonomic API similar to GLSL matrices while using fast Q32 fixed-point arithmetic.

## Phases

1. Implement Mat2Q32 with all operations
2. Implement Mat3Q32 with all operations
3. Implement Mat4Q32 with all operations
4. Add comprehensive tests for all matrix types
5. Cleanup and finalization

## Success Criteria

- All three matrix types (Mat2Q32, Mat3Q32, Mat4Q32) are implemented
- All methods from the reference implementation are included
- Matrix-vector multiplication integrates with Vec2Q32, Vec3Q32, Vec4Q32
- Comprehensive tests pass
- Code is `no_std` compatible
- Code follows project conventions (formatting, organization)
- All warnings are fixed
- Code compiles without errors

# Plan: Convert psrdnoise functions to use vector helpers and add missing functions

## Overview

Refactor psrdnoise implementations (psrdnoise2_q32.rs and psrdnoise3_q32.rs) to use vector helper types (Vec2Q32, Vec3Q32, Vec4Q32) instead of manually expanded component operations. Add missing GLSL-style functions to make the Rust code match the original GLSL as closely as possible.

This will make the code:

- More readable and maintainable
- Closer to the original GLSL structure
- Easier to port future GLSL code
- More compact (estimated 30-40% code reduction)

## Phases

1. Add standalone helper functions (floor, fract, step, min, max, mod, sin, cos, sqrt)
2. Add methods to Vec2Q32
3. Add methods to Vec3Q32 (including extended swizzles)
4. Add methods to Vec4Q32 (including constructor helpers)
5. Refactor psrdnoise2_q32.rs to use helpers
6. Refactor psrdnoise3_q32.rs to use helpers
7. Cleanup and verification

## Success Criteria

- All helper functions implemented and tested
- All wrapper type methods implemented and tested
- psrdnoise2_q32.rs refactored (code reduction ~30-40%)
- psrdnoise3_q32.rs refactored (code reduction ~30-40%)
- All existing tests pass (no functional changes)
- Code matches GLSL structure more closely
- No performance regression (all functions inline)

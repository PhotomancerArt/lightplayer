# Plan: Port HSV Color Space Functions from Lygia

## Overview

Port HSV/HSB color space conversion functions from lygia to lp-glsl-builtins, using Q32 fixed-point
arithmetic and the new q32 vector helpers (Vec3Q32, Vec4Q32). This enables easy porting of GLSL
color manipulation code to Rust.

## Phases

1. Create directory structure and module files
2. Implement saturate_q32 math utility
3. Implement hue2rgb_q32 helper function
4. Implement hsv2rgb_q32 conversion function
5. Implement rgb2hsv_q32 conversion function
6. Add comprehensive tests
7. Cleanup and finalization

## Success Criteria

- All HSV color space functions implemented and working
- Functions follow the same pattern as other lpfx functions
- Helper functions (saturate, hue2rgb) can be inlined
- Tests cover basic conversions, round-trips, edge cases, and epsilon scenarios
- Code compiles without warnings
- Code formatted with `cargo +nightly fmt`
- All tests pass

# Plan: Adapt Lygia FBM Noise to LPFX Function

## Overview

Adapt the Fractal Brownian Motion (FBM) noise function from Lygia to work as an LPFX builtin function. This includes implementing gnoise (gradient noise) and all its dependencies (random, srandom, cubic, quintic interpolation, and mix/lerp functions) to keep the Rust code as close as possible to the original GLSL code.

## Phases

1. Add mix/lerp functions to Q32 and vector types
2. Add cubic and quintic interpolation functions
3. Implement random functions (1D, 2D, 3D)
4. Implement srandom functions (1D, 2D, 3D, and 3D with tiling)
5. Implement gnoise functions (1D, 2D, 3D, and 3D tilable)
6. Implement fbm functions (2D, 3D, and 3D tilable)
7. Regenerate builtin registry
8. Add filetests
9. Cleanup and finalization

## Success Criteria

- All helper functions (mix, cubic, quintic) implemented and tested
- All random functions (random, srandom) implemented and tested
- All gnoise functions implemented and tested
- All fbm functions implemented and tested
- Code structure matches GLSL source closely
- All functions have both q32 and f32 implementations
- All functions registered in builtin system
- Code formatted with `cargo +nightly fmt`
- All tests pass

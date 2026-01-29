# Plan: Create Comprehensive Fragment Processing Function Tests

## Overview

Create a complete test suite for GLSL fragment processing functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/fragment/` following the flat naming convention with prefixes. These tests will comprehensively cover derivative functions (dFdx, dFdy, fwidth) and interpolation functions. These tests are expected to fail initially, serving as a specification for implementing fragment processing function support in the compiler.

## Directory Structure

```javascript
builtins/fragment/
├── dfdx-basic.glsl                (dFdx basic)
├── dfdx-fine.glsl                 (dFdxFine - GLSL)
├── dfdx-coarse.glsl               (dFdxCoarse - GLSL)
├── dfdy-basic.glsl                (dFdy basic)
├── dfdy-fine.glsl                 (dFdyFine - GLSL)
├── dfdy-coarse.glsl               (dFdyCoarse - GLSL)
├── fwidth-basic.glsl              (fwidth basic)
├── fwidth-fine.glsl               (fwidthFine - GLSL)
├── fwidth-coarse.glsl             (fwidthCoarse - GLSL)
├── derivative-scalar.glsl         (derivatives on scalars)
├── derivative-vector.glsl         (derivatives on vectors)
├── derivative-matrix.glsl         (derivatives on matrices)
├── derivative-non-uniform.glsl    (derivatives undefined in non-uniform control flow)
├── derivative-approximation.glsl  (derivatives may be approximate)
├── interpolation-at-sample.glsl   (interpolateAtSample - GLSL)
├── interpolation-at-offset.glsl  (interpolateAtOffset - GLSL)
├── interpolation-at-center.glsl   (interpolateAtCentroid - GLSL)
└── edge-fragment-only.glsl        (fragment shader only)
```

## Key Test Categories

1. **Derivative Functions**: dFdx, dFdy, fwidth (basic, fine, coarse variants)
2. **Interpolation Functions**: interpolateAtSample, interpolateAtOffset, interpolateAtCentroid (GLSL)
3. **Type Support**: Scalars, vectors, matrices
4. **Non-Uniform Control Flow**: Derivatives undefined in non-uniform control flow
5. **Approximation**: Derivatives may be approximate
6. **Fragment Shader Only**: These functions are fragment shader only

## GLSL Spec References

- **builtinfunctions.adoc**: Fragment Processing Functions (lines 3087-3386)
- Key sections: Derivative Functions, Interpolation Functions






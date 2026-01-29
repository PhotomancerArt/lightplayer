# Plan: Create Comprehensive Noise Function Tests

## Overview

Create a complete test suite for GLSL noise functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/noise/` following the flat naming convention with prefixes. These tests will comprehensively cover noise1, noise2, noise3, and noise4 functions. Note: These functions are deprecated and return 0.0 when not generating SPIR-V, and are not declared when generating SPIR-V.

## Directory Structure

```javascript
builtins/noise/
├── noise1-basic.glsl              (noise1 function)
├── noise1-scalar.glsl             (noise1 with scalar input)
├── noise1-vector.glsl             (noise1 with vector input)
├── noise2-basic.glsl              (noise2 function)
├── noise2-scalar.glsl             (noise2 with scalar input)
├── noise2-vector.glsl             (noise2 with vector input)
├── noise3-basic.glsl              (noise3 function)
├── noise3-scalar.glsl             (noise3 with scalar input)
├── noise3-vector.glsl             (noise3 with vector input)
├── noise4-basic.glsl              (noise4 function)
├── noise4-scalar.glsl             (noise4 with scalar input)
├── noise4-vector.glsl             (noise4 with vector input)
├── deprecated-behavior.glsl       (deprecated - returns 0.0)
├── spirv-not-declared.glsl        (not declared when generating SPIR-V)
└── edge-non-constant.glsl         (not compile-time constant)
```

## Key Test Categories

1. **Noise Functions**: noise1, noise2, noise3, noise4
2. **Input Types**: Scalar and vector inputs
3. **Deprecated Behavior**: Returns 0.0 when not generating SPIR-V
4. **SPIR-V**: Not declared when generating SPIR-V
5. **Non-Constant**: Not compile-time constant expressions

## GLSL Spec References

- **builtinfunctions.adoc**: Noise Functions (lines 3387-3431)
- Key sections: Noise1, Noise2, Noise3, Noise4, Deprecation






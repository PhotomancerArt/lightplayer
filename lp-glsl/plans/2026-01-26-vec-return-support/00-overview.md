# Plan: Add Vector Return Support for LPFX Functions

## Overview

Add support for vector return types (Vec2, Vec3, Vec4) in LPFX functions using StructReturn. Currently, LPFX functions only support scalar returns, causing a panic when encountering vector return types like `vec3 lpfx_hue2rgb(float hue)`.

## Phases

1. Update signature building for vector returns
2. Update codegen to handle StructReturn in LPFX calls
3. Update extern C wrappers to use StructReturn
4. Test and verify

## Success Criteria

- LPFX functions with vector return types compile without errors
- GLSL filetests for HSV functions pass
- Existing scalar return functions continue to work
- Both Decimal and NonDecimal implementations support vector returns

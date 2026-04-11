# Plan: Out Parameter Support

## Overview

Implement support for `out` and `inout` parameter qualifiers in GLSL functions. This enables functions to write values back to caller variables through parameters, which is needed for user-defined functions and native LPFX functions like `psrdnoise`.

## Phases

1. Update function signature generation for out/inout parameters
2. Update function call codegen for out/inout arguments
3. Update function definition codegen for out/inout parameters
4. Add lvalue validation for out/inout arguments
5. Update LPFX function signatures for out parameters
6. Review and enhance tests
7. Cleanup and finalization

## Success Criteria

- All existing out/inout parameter tests pass (`param-out.glsl`, `param-inout.glsl`, `param-mixed.glsl`, `edge-lvalue-out.glsl`)
- Out/inout parameters are passed as pointers in function signatures
- Function calls correctly pass addresses and copy back values
- Function definitions correctly handle pointer parameters
- Lvalue validation catches non-lvalue arguments at compile time
- LPFX functions support out parameters
- Code compiles without warnings
- All tests pass

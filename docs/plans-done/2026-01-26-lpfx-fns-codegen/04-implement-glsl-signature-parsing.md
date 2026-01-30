# Phase 4: Implement GLSL signature parsing

## Description

Parse GLSL signature strings into `FunctionSignature` structures using the GLSL parser and existing conversion utilities.

## Implementation

1. Add dependency on `lp-glsl-compiler` crate (or extract utilities)
2. Implement `parse_glsl_signature()` function:
   - Wrap signature string in a function call: `void wrapper() { func(); }`
   - Parse using `glsl::parser::Parse`
   - Extract `FunctionPrototype` from parsed AST
   - Convert to `FunctionSignature` using `function_signature::extract_function_signature()`
3. Handle all GLSL types (float, vec2, vec3, uint, etc.)

## Success Criteria

- Correctly parses simple signatures (e.g., `"u32 lpfx_hash1(u32 x, u32 seed)"`)
- Correctly parses vector signatures (e.g., `"float lpfx_snoise3(vec3 p, u32 seed)"`)
- Returns appropriate error for invalid GLSL syntax
- Code compiles

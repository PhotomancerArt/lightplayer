# Phase 5: Implement validation logic

## Description

Validate discovered LPFX functions for consistency, proper pairing, and correctness.

## Implementation

1. Create `lp-glsl-builtin-gen-app/src/lpfx/validate.rs`
2. Implement validation functions:
    - `validate_lpfx_functions()` - main validation entry point
    - `validate_decimal_pairs()` - ensure all decimal functions have f32/q32 pairs
    - `validate_signature_consistency()` - ensure f32 and q32 signatures match
    - `validate_builtin_ids()` - ensure BuiltinId references are valid
3. Group functions by GLSL function name for pairing
4. Check for:
    - Missing attributes
    - Decimal functions missing f32 or q32 variant
    - f32 and q32 signatures with same name but different signatures
    - Invalid BuiltinId references

## Success Criteria

- Detects missing decimal pairs
- Detects signature mismatches
- Detects invalid BuiltinId references
- Provides clear error messages with context
- Code compiles

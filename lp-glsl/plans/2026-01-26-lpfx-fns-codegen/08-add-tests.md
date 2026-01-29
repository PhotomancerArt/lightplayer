# Phase 8: Add tests for parsing and validation

## Description

Add comprehensive tests for attribute parsing, GLSL signature parsing, and validation logic.

## Implementation

1. Create `lp-builtin-gen/tests/` directory (or add inline tests)
2. Add tests for attribute parsing:
   - Valid non-decimal attribute
   - Valid decimal f32 attribute
   - Valid decimal q32 attribute
   - Invalid syntax cases
3. Add tests for GLSL signature parsing:
   - Simple signatures
   - Vector signatures
   - Invalid GLSL syntax
4. Add tests for validation:
   - Missing pair detection
   - Signature mismatch detection
   - Invalid BuiltinId detection

## Success Criteria

- Tests cover all parsing cases
- Tests cover all validation cases
- Tests cover error cases
- All tests pass
- Code compiles

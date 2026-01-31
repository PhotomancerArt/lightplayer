# Plan: Auto-generate lpfx_fns.rs

## Overview

Extend `lp-glsl-builtin-gen-app` to automatically generate `lpfx_fns.rs` by discovering LPFX
functions annotated with `#[lpfx_impl(...)]` attributes, parsing their GLSL signatures, and
generating the registry code.

## Phases

1. Set up module structure and error types
2. Implement discovery of LPFX functions with attributes
3. Implement attribute parsing
4. Implement GLSL signature parsing
5. Implement validation logic
6. Implement code generation
7. Integrate into main codegen flow
8. Add tests for parsing and validation
9. Update existing LPFX functions with attributes
10. Cleanup and finalization

## Success Criteria

- All LPFX functions have `#[lpfx_impl(...)]` attributes
- Codegen successfully discovers and parses all LPFX functions
- Generated `lpfx_fns.rs` matches current manual implementation structure
- All validation errors are caught with clear messages
- Tests cover parsing, validation, and error cases
- Generated code compiles and works correctly
- No manual maintenance of `lpfx_fns.rs` required

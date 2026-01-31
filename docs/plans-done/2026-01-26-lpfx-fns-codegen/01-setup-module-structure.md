# Phase 1: Set up module structure and error types

## Description

Create the module structure for LPFX codegen and define error types for clear error reporting.

## Implementation

1. Create `lp-glsl-builtin-gen-app/src/lpfx/mod.rs` with module declarations
2. Create `lp-glsl-builtin-gen-app/src/lpfx/errors.rs` with error types:
    - `LpfxCodegenError` enum with variants for different error cases
    - Helper functions for creating error messages
3. Create placeholder files for other modules (`parse.rs`, `validate.rs`, `generate.rs`)

## Success Criteria

- Module structure exists
- Error types defined with clear variants
- Error messages include context (function name, file path, etc.)
- Code compiles

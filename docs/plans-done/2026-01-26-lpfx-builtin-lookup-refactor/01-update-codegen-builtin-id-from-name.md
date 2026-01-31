# Phase 1: Update Codegen Tool to Generate `builtin_id_from_name` Function

## Description

Add code generation for `builtin_id_from_name` function in `registry.rs`. This function provides
reverse lookup of `BuiltinId::name()`.

## Implementation

1. Update `lp-glsl/lp-glsl-builtin-gen-app/src/main.rs`:
    - Add function to generate `builtin_id_from_name` match statement
    - Insert generated function into `registry.rs` after `BuiltinId::name()` method
    - Use same pattern as `BuiltinId::name()` but reverse (name → variant)

2. The generated function should:
    - Match on all builtin names (both f32 and q32 variants)
    - Return `Some(BuiltinId::Variant)` for known names
    - Return `None` for unknown names

## Success Criteria

- Codegen tool generates `builtin_id_from_name` function
- Function includes all builtin IDs (both f32 and q32 variants)
- Function compiles without errors
- Function is inserted in correct location in `registry.rs`

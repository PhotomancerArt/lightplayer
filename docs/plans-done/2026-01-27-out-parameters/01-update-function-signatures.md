# Phase 1: Update Function Signature Generation

## Description

Update function signature generation to pass out/inout parameters as pointers instead of by value.
This affects both user-defined functions and LPFX functions.

## Implementation

### Files to Modify

1. **`lp-glsl/lp-glsl-compiler/src/frontend/codegen/signature.rs`**
    - Update `add_parameters()` to check `param.qualifier`
    - Update `add_type_as_params()` to accept qualifier parameter
    - For `Out`/`InOut`: Add pointer type parameter
    - For `In`: Continue existing behavior (expand to components)

### Changes

1. **`SignatureBuilder::add_parameters()`**
    - Iterate through parameters with qualifiers
    - Call `add_type_as_params()` with qualifier information

2. **`SignatureBuilder::add_type_as_params()`**
    - Add parameter: `qualifier: ParamQualifier`
    - If `Out` or `InOut`: Add single `pointer_type` parameter
    - If `In`: Expand to components as before (vectors/matrices → multiple params)

### Success Criteria

- Function signatures include pointer parameters for out/inout
- Function signatures continue to expand in parameters to components
- Code compiles without errors
- No test changes needed yet (tests will fail until later phases)

## Notes

- Use `pointer_type` from ISA (available via `CodegenContext`)
- For vectors/matrices with out/inout: Pass single pointer to first element
- Maintain backward compatibility for in parameters

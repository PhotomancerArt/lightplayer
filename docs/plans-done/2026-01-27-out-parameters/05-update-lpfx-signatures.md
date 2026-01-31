# Phase 5: Update LPFX Function Signatures

## Description

Update LPFX function signature building to support out parameters. This enables native functions
like `psrdnoise` to use out parameters.

## Implementation

### Files to Modify

1. **`lp-glsl/lp-glsl-compiler/src/frontend/semantic/lpfx/lpfx_sig.rs`**
    - Update `build_call_signature()` to check parameter qualifiers
    - Update `convert_to_cranelift_types()` to handle qualifiers

### Changes

1. **`build_call_signature()`**
    - Check `func.glsl_sig.parameters` for qualifiers
    - For out/inout parameters: Add pointer type to signature
    - For in parameters: Continue existing behavior (expand to components)

2. **`convert_to_cranelift_types()`**
    - Add parameter: `qualifiers: &[ParamQualifier]` (or iterate with parameters)
    - For out/inout: Return pointer type
    - For in: Return value types as before (expand vectors/matrices)

3. **`expand_vector_args()`**
    - May need updates if out parameters are passed differently
    - Check if this function is used for LPFX calls

### Success Criteria

- LPFX function signatures include pointer parameters for out/inout
- LPFX function calls compile with out parameters
- Existing LPFX functions without out parameters continue to work
- Code compiles without errors

## Notes

- Only LPFX functions need out parameter support (not all native functions)
- For vector out parameters: Single pointer to first element (like arrays)
- Native functions receive pointers and write to them directly
- May need to check how LPFX function calls are generated (similar to user functions)

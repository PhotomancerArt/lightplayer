# Phase 6: Implement code generation

## Description

Generate `lpfx_fns.rs` source code with `init_functions()` containing all `LpfxFn` structures.

## Implementation

1. Create `lp-builtin-gen/src/lpfx/generate.rs`
2. Implement `generate_lpfx_fns()` function:
   - Generate `lpfx_fns()` function with caching logic
   - Generate `init_functions()` that returns array
   - Generate `LpfxFn` structures for each function:
     - `glsl_sig` with `FunctionSignature`
     - `impls` with `LpfxFnImpl::NonDecimal` or `LpfxFnImpl::Decimal`
3. Group functions by GLSL name and pair f32/q32 variants
4. Generate proper Rust code formatting

## Success Criteria

- Generates code matching current `lpfx_fns.rs` structure
- Correctly generates non-decimal functions
- Correctly generates decimal functions with pairs
- Generated code compiles
- Code compiles

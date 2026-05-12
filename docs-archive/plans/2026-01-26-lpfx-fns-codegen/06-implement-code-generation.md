# Phase 6: Implement code generation

## Description

Generate `lpfn_fns.rs` source code with `init_functions()` containing all `LpfnFn` structures.

## Implementation

1. Create `lps-builtin-gen-app/src/lpfn/generate.rs`
2. Implement `generate_lpfn_fns()` function:
    - Generate `lpfn_fns()` function with caching logic
    - Generate `init_functions()` that returns array
    - Generate `LpfnFn` structures for each function:
        - `glsl_sig` with `FunctionSignature`
        - `impls` with `LpfnFnImpl::NonDecimal` or `LpfnFnImpl::Decimal`
3. Group functions by GLSL name and pair f32/q32 variants
4. Generate proper Rust code formatting

## Success Criteria

- Generates code matching current `lpfn_fns.rs` structure
- Correctly generates non-decimal functions
- Correctly generates decimal functions with pairs
- Generated code compiles
- Code compiles

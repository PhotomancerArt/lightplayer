# Design: Out Parameter Support

## Overview

Implement support for `out` and `inout` parameter qualifiers in GLSL functions. This enables
functions to write values back to caller variables through parameters, which is needed for
user-defined functions and native LPFX functions like `psrdnoise`.

## Architecture

### Parameter Passing Strategy

- **In parameters** (default): Passed by value (expanded to components for vectors/matrices)
- **Out/inout parameters**: Passed as pointers (single pointer per parameter, regardless of type)
    - For vectors/matrices: Pass pointer to first element (like arrays)
    - Use `pointer_type` from ISA for all out/inout parameters

### Function Call Flow

1. **Argument Preparation**: For out/inout arguments, resolve as lvalue and get address
2. **Call**: Pass pointer as argument (or value for in parameters)
3. **Copy-back**: After call completes, load values from pointer and store back to original lvalue
4. **Order**: Copy back in parameter order (left to right)

### Function Definition Flow

1. **Parameter Declaration**: Out/inout parameters arrive as pointers in block parameters
2. **Storage**: Store pointer (don't load value initially)
3. **Reading**: Load from pointer when reading (for inout, also for initial value of out)
4. **Writing**: Store to pointer when writing

### LValue Validation

- Validate at compile-time in semantic checking phase
- Use existing `resolve_lvalue()` function - if it succeeds, argument is valid lvalue
- Emit error if non-lvalue passed to out/inout parameter

## File Structure

```
lp-glsl/lp-glsl-compiler/src/
├── frontend/
│   ├── codegen/
│   │   ├── signature.rs                    # UPDATE: Check qualifiers, pass pointers for out/inout
│   │   ├── expr/
│   │   │   └── function.rs                  # UPDATE: Handle out/inout in calls (address, copy-back)
│   │   └── lvalue/
│   │       └── resolve/mod.rs               # REUSE: For lvalue validation
│   ├── semantic/
│   │   ├── passes/
│   │   │   └── function_signature.rs        # REUSE: Already parses qualifiers
│   │   └── functions.rs                     # REUSE: Already stores qualifiers
│   └── glsl_compiler.rs                     # UPDATE: Handle out/inout in definitions (pointer storage)
└── frontend/semantic/lpfx/
    └── lpfx_sig.rs                          # UPDATE: Support out parameters in LPFX signatures
```

## Type and Function Summary

### signature.rs

**UPDATE**: `SignatureBuilder::add_parameters()`

- Check `param.qualifier` for each parameter
- For `Out`/`InOut`: Add pointer type parameter instead of value types
- For `In`: Continue existing behavior (expand to components)

**UPDATE**: `SignatureBuilder::add_type_as_params()`

- New parameter: `qualifier: ParamQualifier`
- If `Out`/`InOut`: Add single pointer parameter
- If `In`: Expand to components as before

### function.rs

**UPDATE**: `prepare_call_arguments()`

- For out/inout parameters: Resolve argument as lvalue, get address
- Pass pointer as argument
- Track which arguments are out/inout for copy-back

**NEW**: `copy_back_out_parameters()`

- After function call completes
- For each out/inout parameter: Load from pointer, store to original lvalue
- Copy back in parameter order

**UPDATE**: `emit_user_function_call()`

- Call `copy_back_out_parameters()` after `execute_function_call()`

### glsl_compiler.rs

**UPDATE**: Parameter declaration section

- For out/inout parameters: Store pointer from block parameter (don't load value)
- For in parameters: Continue existing behavior (load values)

**UPDATE**: Variable declaration for parameters

- Out/inout parameters: Store as pointer variable
- When parameter is accessed: Load/store from pointer

### lpfx_sig.rs

**UPDATE**: `build_call_signature()`

- Check parameter qualifiers from `func.glsl_sig.parameters`
- For out/inout: Add pointer type to signature
- For in: Continue existing behavior

**UPDATE**: `convert_to_cranelift_types()`

- New parameter: `qualifiers: &[ParamQualifier]`
- For out/inout: Return pointer type
- For in: Return value types as before

### Semantic Validation

**NEW**: `validate_out_inout_arguments()`

- In semantic checking phase (before codegen)
- For each out/inout parameter: Try to resolve argument as lvalue
- Emit error if `resolve_lvalue()` fails

## Implementation Details

### Pointer Handling for Vectors/Matrices

For out/inout vector/matrix parameters:

- Pass single pointer to first element
- In function body: Use pointer arithmetic for component access
- Component access (e.g., `result.x`): Compute offset, load/store at offset

### Copy-Back Implementation

After function call:

```rust
// For each out/inout parameter:
let pointer = call_args[param_idx];
let component_count = param.ty.component_count();
for i in 0..component_count {
    let offset = i * element_size_bytes;
    let value = load(pointer, offset);
    store_to_lvalue(original_lvalue, i, value);
}
```

### Parameter Storage in Function Body

For out/inout parameters:

- Store pointer in variable (not values)
- When reading: `load(pointer, offset)`
- When writing: `store(pointer, offset, value)`

For in parameters:

- Continue existing behavior (values stored in variables)

## Edge Cases

1. **Uninitialized out parameters**: Start uninitialized (undefined behavior to read before write)
2. **Multiple out parameters**: Copy back in order (left to right)
3. **Out parameter aliasing**: If same variable passed twice, both get updated (GLSL allows this)
4. **Array out parameters**: Pass pointer to first element, use pointer arithmetic for indexing

## Testing Strategy

- Review existing tests: `param-out.glsl`, `param-inout.glsl`, `param-mixed.glsl`,
  `edge-lvalue-out.glsl`
- Add tests for:
    - Array out/inout parameters
    - Multiple out parameters
    - Out parameter aliasing
    - Error cases: non-lvalue arguments
    - Vector/matrix out parameters
    - LPFX function out parameters

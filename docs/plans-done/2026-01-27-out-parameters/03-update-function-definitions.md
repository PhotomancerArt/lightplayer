# Phase 3: Update Function Definition Codegen

## Description

Update function definition codegen to handle out/inout parameters that arrive as pointers. Parameters should be stored as pointers and accessed via load/store operations.

## Implementation

### Files to Modify

1. **`lp-glsl/crates/lp-glsl-compiler/src/frontend/glsl_compiler.rs`**
   - Update parameter declaration section
   - Handle out/inout parameters as pointers
   - Update parameter access to load/store from pointers

2. **`lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/read.rs`**
   - Handle reading from out/inout parameter variables (load from pointer)

3. **`lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/write.rs`**
   - Handle writing to out/inout parameter variables (store to pointer)

### Changes

1. **Parameter Declaration (`glsl_compiler.rs`)**
   - For out/inout parameters: Store pointer from block parameter (don't load value)
   - For in parameters: Continue existing behavior (load values into variables)
   - Need to track which parameters are out/inout in variable storage

2. **Parameter Variable Storage**
   - Out/inout parameters: Store pointer value in variable (or special storage)
   - In parameters: Store values as before
   - May need to extend `VarInfo` or add separate tracking for pointer parameters

3. **Parameter Access (read.rs/write.rs)**
   - When reading out/inout parameter: Load from pointer
   - When writing out/inout parameter: Store to pointer
   - For component access (e.g., `result.x`): Compute offset, load/store at offset
   - For in parameters: Continue existing behavior

### Success Criteria

- Function definitions with out/inout parameters compile
- Out/inout parameters are stored as pointers
- Reading from out/inout parameters loads from pointer
- Writing to out/inout parameters stores to pointer
- Component access works for out/inout parameters
- Existing in-parameter functions continue to work
- Code compiles without errors

## Notes

- Out parameters start uninitialized (undefined behavior to read before write)
- Inout parameters: Load initial value from pointer when reading
- Component access: Use pointer arithmetic (offset = component_index \* element_size)
- Vector/matrix out parameters: Single pointer to first element, use offsets for components

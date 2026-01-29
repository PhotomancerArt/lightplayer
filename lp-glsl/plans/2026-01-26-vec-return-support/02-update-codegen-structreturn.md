# Phase 2: Update Codegen to Handle StructReturn in LPFX Calls

## Description

Update `emit_lp_lib_fn_call()` in `lpfx_fns.rs` to handle StructReturn for vector returns. This includes allocating a stack slot, passing the pointer, and loading return values after the call.

## Changes

### `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lpfx_fns.rs`

- Update `emit_lp_lib_fn_call()` to:
  - Check if function uses StructReturn (check signature)
  - If StructReturn: allocate stack slot for return buffer
  - Pass StructReturn pointer as first argument to call
  - After call: load return values from buffer at offsets
  - Return loaded values as vector components
- Handle both Decimal and NonDecimal implementations
- Use existing patterns from user function calls (see `function.rs`)

## Success Criteria

- StructReturn buffer is allocated before call
- StructReturn pointer is passed as first argument
- Return values are loaded from buffer after call
- Both Decimal and NonDecimal paths handle StructReturn
- Scalar returns continue to work as before

## Implementation Notes

- Use `setup_struct_return_buffer()` pattern from user functions
- Buffer size: component_count × 4 bytes
- Load offsets: 0, 4, 8, 12 bytes for components 0, 1, 2, 3
- Use `MemFlags::trusted()` for loads

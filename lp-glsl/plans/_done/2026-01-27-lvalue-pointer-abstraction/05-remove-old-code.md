# Phase 5: Remove Old Code

## Description

Remove the old code paths: `name` fields from `Variable` and `Component` variants, runtime lookups in read/write functions, and `out_inout_ptr` from `VarInfo`. This completes the refactoring.

## Success Criteria

- [ ] `name` field removed from `Variable` variant
- [ ] `name` field removed from `Component` variant
- [ ] Runtime lookups removed from `read_lvalue()`
- [ ] Runtime lookups removed from `write_lvalue()`
- [ ] `out_inout_ptr` removed from `VarInfo`
- [ ] All code that referenced removed fields updated
- [ ] Code compiles without errors
- [ ] All tests pass

## Implementation Notes

### Files to Modify

- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/types.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/read.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/write.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/context.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/glsl_compiler.rs`

### Changes

1. **Remove `name` field from `LValue` variants**:
   ```rust
   // Before:
   Variable {
       vars: Vec<Variable>,
       ty: GlslType,
       name: Option<String>,
   }
   
   // After:
   Variable {
       vars: Vec<Variable>,
       ty: GlslType,
   }
   ```

2. **Remove runtime lookups from `read_lvalue()`**:
   - Remove the `if let Some(var_name) = name { ... }` blocks
   - Remove VarInfo lookups
   - Keep only the SSA variable access path

3. **Remove runtime lookups from `write_lvalue()`**:
   - Same as read_lvalue - remove name-based pointer detection
   - Keep only SSA variable access path

4. **Remove `out_inout_ptr` from `VarInfo`**:
   ```rust
   // Before:
   pub struct VarInfo {
       pub cranelift_vars: Vec<Variable>,
       pub glsl_type: GlslType,
       pub array_ptr: Option<Value>,
       pub stack_slot: Option<StackSlot>,
       pub out_inout_ptr: Option<Value>,  // Remove this
   }
   ```

5. **Update parameter declaration code**:
   - Remove code that sets `out_inout_ptr` in `VarInfo`
   - Pointer is now stored directly in `LValue::PointerBased`

### Verification

- Search for any remaining references to `name` field
- Search for any remaining `out_inout_ptr` accesses
- Ensure no dead code remains
- Run all tests to verify nothing broke

### Code Organization

- Remove all temporary migration code
- Clean up any commented-out code
- Ensure consistent formatting

### Formatting

- Run `cargo +nightly fmt` on entire workspace

### Language and Tone

- Use measured, factual descriptions
- Note that refactoring is complete, not that code is "perfect"

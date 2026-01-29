# Phase 2: Update Out/Inout Resolution

## Description

Modify variable and component resolution functions to create `PointerBased` LValues for out/inout parameters instead of using the `name` field approach. Keep the old code path temporarily for safety.

## Success Criteria

- [ ] `resolve_variable_lvalue()` creates `PointerBased` for out/inout parameters
- [ ] Component resolution creates `PointerBased` for out/inout component access
- [ ] Old code path still works (both paths active)
- [ ] Code compiles without errors
- [ ] Existing tests still pass

## Implementation Notes

### Files to Modify

- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/variable.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/variable.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/mod.rs`

### Changes

1. **Update `resolve_variable_lvalue()`**:
   - When `is_out_inout` is true, get pointer from `var_info.out_inout_ptr`
   - Create `LValue::PointerBased` with `Direct` pattern instead of `Variable` with `name`
   - Keep old code path for non-out/inout variables

2. **Update `resolve_component_on_variable()`**:
   - Accept pointer parameter (or get from context)
   - If pointer exists, create `PointerBased` with `Component` pattern
   - Otherwise, create regular `Component` variant

3. **Update component resolution in `mod.rs`**:
   - When resolving component on out/inout variable, pass pointer through
   - Create `PointerBased` variant instead of `Component` with `name`

### Example Changes

```rust
// In resolve_variable_lvalue():
if is_out_inout {
    let ptr = var_info.out_inout_ptr.expect("out/inout param must have pointer");
    let component_count = if ty.is_vector() {
        ty.component_count().unwrap()
    } else if ty.is_matrix() {
        ty.matrix_element_count().unwrap()
    } else {
        1
    };
    return Ok(LValue::PointerBased {
        ptr,
        base_ty: ty,
        access_pattern: PointerAccessPattern::Direct { component_count },
    });
}
```

### Code Organization

- Keep new code alongside old code during migration
- Add comments indicating temporary dual-path approach
- Place helper functions at bottom of files

### Formatting

- Run `cargo +nightly fmt` on changes before committing

### Language and Tone

- Use measured, factual descriptions
- Note that this is incremental migration, not final state

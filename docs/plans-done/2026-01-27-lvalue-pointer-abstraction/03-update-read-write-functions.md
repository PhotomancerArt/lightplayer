# Phase 3: Update Read/Write Functions

## Description

Add handling for the `PointerBased` variant in `read_lvalue()` and `write_lvalue()` functions. Keep
the old code path for `Variable` and `Component` variants with name lookup temporarily.

## Success Criteria

- [ ] `read_lvalue()` handles `PointerBased` variant for all access patterns
- [ ] `write_lvalue()` handles `PointerBased` variant for all access patterns
- [ ] Old code path still works (both paths active)
- [ ] Code compiles without errors
- [ ] All existing tests pass
- [ ] New `PointerBased` LValues work correctly

## Implementation Notes

### Files to Modify

- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/read.rs`
- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/write.rs`

### Changes

1. **Add `PointerBased` match arm to `read_lvalue()`**:
    - Handle `Direct` pattern: load all components
    - Handle `Component` pattern: load only requested components
    - Handle `ArrayElement` pattern: calculate offset and load element(s)

2. **Add `PointerBased` match arm to `write_lvalue()`**:
    - Handle `Direct` pattern: store all components
    - Handle `Component` pattern: store only requested components
    - Handle `ArrayElement` pattern: calculate offset and store element(s)

### Implementation Details

#### Direct Pattern

```rust
PointerAccessPattern::Direct { component_count } => {
    let base_cranelift_ty = /* determine from base_ty */;
    let component_size_bytes = base_cranelift_ty.bytes() as usize;
    let flags = cranelift_codegen::ir::MemFlags::trusted();
    for i in 0..component_count {
        let offset = (i * component_size_bytes) as i32;
        let val = ctx.builder.ins().load(base_cranelift_ty, flags, ptr, offset);
        vals.push(val);
    }
}
```

#### Component Pattern

```rust
PointerAccessPattern::Component { indices, .. } => {
    let base_cranelift_ty = base_ty.vector_base_type().unwrap().to_cranelift_type()?;
    let component_size_bytes = base_cranelift_ty.bytes() as usize;
    let flags = cranelift_codegen::ir::MemFlags::trusted();
    for &idx in indices {
        let offset = (idx * component_size_bytes) as i32;
        let val = ctx.builder.ins().load(base_cranelift_ty, flags, ptr, offset);
        vals.push(val);
    }
}
```

#### ArrayElement Pattern

- Similar to existing `ArrayElement` variant logic
- Calculate element offset (compile-time or runtime)
- Handle component access if `component_indices` is Some
- Load/store entire element or specific components

### Code Organization

- Place `PointerBased` match arm before `Variable` and `Component` arms
- Extract common offset calculation logic to helper functions
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on changes before committing

### Language and Tone

- Use measured, factual descriptions
- Note that old code path remains for safety during migration

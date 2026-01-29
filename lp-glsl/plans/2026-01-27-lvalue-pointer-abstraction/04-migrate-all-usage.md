# Phase 4: Migrate All Usage

## Description

Update all LValue creation sites to use `PointerBased` for out/inout parameters. Ensure all out/inout params use the new variant, and remove `name` field usage for pointer detection.

## Success Criteria

- [ ] All out/inout parameter LValues use `PointerBased` variant
- [ ] All out/inout component access uses `PointerBased` variant
- [ ] `name` field no longer used for pointer detection
- [ ] Code compiles without errors
- [ ] All tests pass
- [ ] No regressions in functionality

## Implementation Notes

### Files to Modify

- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/variable.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/variable.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/mod.rs`
- Any other sites that create LValues for out/inout parameters

### Changes

1. **Remove `name` field usage**:
   - Stop setting `name: Some(...)` for out/inout parameters
   - Remove checks for `name` field to detect out/inout
   - All out/inout detection now happens via `PointerBased` variant

2. **Update all creation sites**:
   - Ensure `resolve_variable_lvalue()` always creates `PointerBased` for out/inout
   - Ensure component resolution always creates `PointerBased` for out/inout components
   - Check for any other places that create LValues for out/inout params

3. **Out/inout array parameters**:
   - If arrays as out/inout params are encountered, use `PointerBased` with `Direct` pattern
   - Array element access (`arr[i]`) continues to use `LValue::ArrayElement` for now

### Verification

- Search codebase for `name: Some(` to find remaining usages
- Search for `out_inout_ptr` accesses to ensure all migrated
- Run all filetests, especially those with out/inout parameters

### Code Organization

- Remove commented-out old code
- Clean up temporary migration code
- Keep code organized and readable

### Formatting

- Run `cargo +nightly fmt` on changes before committing

### Language and Tone

- Use measured, factual descriptions
- Note completion of migration, not "perfect" or "complete" solution

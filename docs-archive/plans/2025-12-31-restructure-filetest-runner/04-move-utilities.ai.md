# Phase 4: Move Utilities to util/ Module

## Tasks

1. Move utility code to `util/` module:
   - Move `file_update.rs` → `util/file_update.rs`
   - Move `validation.rs` → `util/validation.rs`
   - Create `util/path.rs` - Extract path utilities from lib.rs

2. Update `util/mod.rs` to re-export all utilities

3. Update all imports throughout codebase

4. Move `test_compile.rs` → `test_compile/compile.rs`
5. Move `test_transform.rs` → `test_transform/transform.rs`

## Files to Create

- `src/util/path.rs`

## Files to Move

- `src/file_update.rs` → `src/util/file_update.rs`
- `src/validation.rs` → `src/util/validation.rs`
- `src/test_compile.rs` → `src/test_compile/compile.rs`
- `src/test_transform.rs` → `src/test_transform/transform.rs`

## Files to Modify

- `src/util/mod.rs` - Add re-exports
- `src/test_compile/mod.rs` - Add re-exports
- `src/test_transform/mod.rs` - Add re-exports
- All files that import utilities - Update imports

## Files to Remove

- `src/file_update.rs` (after move)
- `src/validation.rs` (after move)
- `src/test_compile.rs` (after move)
- `src/test_transform.rs` (after move)

## Success Criteria

- All utilities moved to `util/` module
- Test compile and transform modules organized
- Code compiles
- No warnings
- **Tests added for utility functions:**
  - `util/file_update::format_glsl_value()` - test value formatting
  - `util/file_update::FileUpdate` - test file update operations
- **All tests pass before moving to Phase 5**


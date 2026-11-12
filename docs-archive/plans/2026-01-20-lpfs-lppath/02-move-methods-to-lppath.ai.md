# Phase 2: Move read-only methods from LpPathBuf to LpPath

## Description

Move all read-only methods from `LpPathBuf` to `LpPath`. These methods don't mutate, so they belong on the slice type. `LpPathBuf` will get access to them via `Deref` in the next phase.

## Implementation

1. Update `lp-model/src/path.rs`:
   - Move these methods from `impl LpPathBuf` to `impl LpPath`:
     - `is_absolute()` → returns `bool`
     - `is_relative()` → returns `bool`
     - `parent()` → returns `Option<&LpPath>` (borrowed view)
     - `file_name()` → returns `Option<&str>`
     - `file_stem()` → returns `Option<&str>`
     - `extension()` → returns `Option<&str>`
     - `strip_prefix()` → returns `Option<&LpPath>` (borrowed view)
     - `starts_with()` → returns `bool`
     - `ends_with()` → returns `bool`
     - `components()` → returns `Components<'_>`
   - Update method implementations to work with `&LpPath` instead of `&LpPathBuf`
   - Update return types: `parent()` and `strip_prefix()` return `Option<&LpPath>` instead of `Option<LpPathBuf>`

2. Keep mutation methods on `LpPathBuf`:
   - `join()` → returns `LpPathBuf`
   - `join_relative()` → returns `Option<LpPathBuf>`
   - Any other mutation methods

## Success Criteria

- All read-only methods moved to `LpPath`
- Methods compile and work correctly
- `parent()` and `strip_prefix()` return `&LpPath` (borrowed views)
- Code compiles without errors
- Tests updated and passing

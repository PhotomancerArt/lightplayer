# Phase 5: Remove normalize_path() functions

## Description

Remove all `normalize_path()` functions from `LpFs` implementations. This will cause compilation errors that we'll fix in the next phase. The normalization is now handled by `LpPathBuf::from()`.

## Implementation

1. Update `lp-shared/src/fs/lp_fs_mem.rs`:
   - Remove `normalize_path()` function
   - Remove all calls to `Self::normalize_path()`

2. Update `lp-shared/src/fs/lp_fs_std.rs`:
   - Remove `normalize_path()` function (if it exists)
   - Remove all calls to normalization functions

3. Update `lp-shared/src/fs/lp_fs_view.rs`:
   - Remove `normalize_path()` function
   - Remove all calls to `Self::normalize_path()`

## Success Criteria

- All `normalize_path()` functions removed
- Code will NOT compile (expected - we'll fix in next phase)
- No normalization logic remains in implementations

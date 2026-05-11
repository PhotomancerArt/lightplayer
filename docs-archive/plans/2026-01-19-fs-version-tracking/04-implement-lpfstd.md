# Phase 4: Implement version tracking in LpFsStd

## Description

Implement version tracking in `LpFsStd`. It should track changes via `record_changes()` calls from external sources (like `FileWatcher`).

## Implementation

1. Update `lp-shared/src/fs/lp_fs_std.rs`:
   - Add `current_version: RefCell<FsVersion>` field to `LpFsStd`
   - Add `changes: RefCell<HashMap<String, (FsVersion, ChangeType)>>` field
   - Add `record_change()` helper method to increment version and store in map
   - Implement `current_version()` - returns current version
   - Implement `get_changes_since()` - filters changes by version
   - Implement `clear_changes_before()` - removes old changes
   - Implement `record_changes()` - normalizes paths and records changes

2. Optionally track changes from `write_file()` and `delete_file()` operations (for consistency with `LpFsMemory`)

## Success Criteria

- `LpFsStd` tracks versions via `record_changes()`
- All four trait methods implemented correctly
- Path normalization works correctly in `record_changes()`
- Code compiles without errors

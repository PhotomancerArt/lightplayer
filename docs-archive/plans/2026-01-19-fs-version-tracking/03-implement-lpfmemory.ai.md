# Phase 3: Implement version tracking in LpFsMemory

## Description

Implement version tracking in `LpFsMemory`. It should track changes from its own `write_file` and `delete_file` operations.

## Implementation

1. Update `lp-shared/src/fs/lp_fs_mem.rs`:
   - Add `current_version: RefCell<FsVersion>` field to `LpFsMemory`
   - Add `changes: RefCell<HashMap<String, (FsVersion, ChangeType)>>` field
   - Update `record_change()` to increment version and store in map
   - Implement `current_version()` - returns current version
   - Implement `get_changes_since()` - filters changes by version
   - Implement `clear_changes_before()` - removes old changes
   - Implement `record_changes()` - records external changes (for consistency)

2. Update `write_file()` to call `record_change()` with appropriate `ChangeType`
3. Update `delete_file()` to call `record_change()` with `ChangeType::Delete`
4. Update `delete_dir()` to record deletions for all affected files

5. Remove old `get_changes()` and `reset_changes()` methods (replaced by version-based API)

## Success Criteria

- `LpFsMemory` tracks versions from its own operations
- All four trait methods implemented correctly
- Old change tracking methods removed
- Code compiles without errors
- Existing tests updated to use new API

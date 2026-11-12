# Phase 8: Update fs_loop to record changes in LpFsStd

## Description

Update `fs_loop` to record `FileWatcher` changes in `LpFsStd` so that the client-side filesystem tracks versions.

## Implementation

1. Update `lp-cli/src/commands/dev/fs_loop.rs`:
   - After collecting changes from `FileWatcher` and before syncing to server
   - Check if `local_fs` is `LpFsStd` (may require type checking or trait object downcasting)
   - Call `local_fs.record_changes(changes)` to update version tracking
   - Handle the mutable access requirement (may need `Arc<Mutex<LpFsStd>>` or similar)

**Note**: This may require refactoring `fs_loop` to use `Arc<Mutex<LpFsStd>>` instead of `Arc<dyn LpFs>` to get mutable access. Alternatively, we could pass changes through a different mechanism.

## Success Criteria

- `fs_loop` records changes in `LpFsStd`
- Version tracking works on client side
- Code compiles without errors
- File changes are tracked correctly

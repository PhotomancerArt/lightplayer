# Phase 4: Update handle_fs_changes to update status

## Description

Modify `ProjectRuntime::handle_fs_changes()` to update node status after processing file changes. When a shader's GLSL file changes and compilation succeeds or fails, update the status accordingly and generate `StatusChanged` changes.

## Changes

### File: `lp-engine/src/project/runtime.rs`

1. **Modify `handle_fs_changes()` method**:
   - After calling `runtime.handle_fs_change()`, check if it's a shader runtime
   - If it's a shader runtime, check if it has a compilation error
   - If compilation error exists, update status to `NodeStatus::Error(error_message)`
   - If no compilation error, update status to `NodeStatus::Ok`
   - Generate `StatusChanged` change if status changed
   - Track previous status to detect changes

2. **Status change detection**:
   - Compare new status with previous status
   - Only generate `StatusChanged` if status actually changed
   - This ensures clients get notified of status changes

## Success Criteria

- Status is updated when GLSL files change
- Status changes from `Error` to `Ok` when compilation succeeds
- Status changes from `Ok` to `Error` when compilation fails
- `StatusChanged` changes are generated when status changes
- Code compiles without errors
- Tests pass

## Notes

- This ensures status stays in sync with compilation state
- File changes are processed even when there's a compilation error
- Status changes will be picked up by clients in the next sync

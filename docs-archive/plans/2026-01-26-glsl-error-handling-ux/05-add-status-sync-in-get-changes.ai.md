# Phase 5: Add status synchronization in get_changes

## Description

Modify `ProjectRuntime::get_changes()` to ensure node status is synchronized with runtime state before extracting node details. This ensures status is always current when clients request it, even if status wasn't updated during init or file changes.

## Changes

### File: `lp-engine/src/project/runtime.rs`

1. **Modify `get_changes()` method**:
   - Before extracting node details, check each shader node's runtime state
   - Compare `compilation_error` in runtime with current status
   - If status doesn't match compilation error state, update status
   - Generate `StatusChanged` change if status was updated
   - This ensures status is always current when clients sync

2. **Status synchronization logic**:
   - For shader nodes, check if `compilation_error` exists
   - If error exists and status is not `Error`, update to `Error`
   - If no error and status is `Error`, update to `Ok`
   - Only update if status actually changes

## Success Criteria

- Status is synchronized with runtime state before extracting details
- Status changes are detected and `StatusChanged` changes are generated
- Clients always see current status when syncing
- Code compiles without errors
- Tests pass

## Notes

- This is a safety net to ensure status is always current
- Handles edge cases where status might get out of sync
- Status changes will be picked up by clients in the sync response

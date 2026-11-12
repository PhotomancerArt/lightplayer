# Phase 3: Implement sync mechanism

## Goal

Restructure `project_sync()` and `handle_sync()` to actually perform syncs without holding locks across await points.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`:
   - Restructure `project_sync()` method:
     - Lock view, read `since_frame` and `detail_specifier`, unlock
     - Do async `project_get_changes()` call (no lock held)
     - Lock view, call `apply_changes()` with response, unlock
   - Ensure no lock is held across await points

2. Update `lp-app/apps/lp-cli/src/debug_ui/ui.rs`:
   - Implement `handle_sync()` method:
     - Check if sync is in progress (return early if so)
     - Update `detail_tracking` in view to match `tracked_nodes`
     - Call `project_sync()` with proper lock management
     - Handle sync completion and errors
   - Remove placeholder TODO comments

3. Ensure sync happens automatically:
   - `handle_sync()` is called in UI update loop
   - Only one sync in flight at a time
   - Sync as soon as previous sync completes

## Success Criteria

- `project_sync()` doesn't hold lock across await
- `handle_sync()` actually performs syncs
- Sync happens automatically in UI update loop
- Only one sync in flight at a time
- UI displays updated project state
- Code compiles without errors

## Implementation Notes

- Use `blocking_lock()` for sync operations (UI thread is sync context)
- Release lock before async operations
- Re-acquire lock for view updates
- Handle sync errors gracefully (log and continue)
- Set `sync_in_progress` flag appropriately

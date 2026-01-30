# Phase 8: Integrate UI into dev command

## Goal

Wire up the debug UI to the dev command handler so it spawns when dev command runs.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - After loading project, if not headless:
     - Create `ClientProjectView` instance
     - Create `DebugUiState` with project view and handle
     - Spawn egui window with `eframe::run_native()`
     - Pass `AsyncLpClient` to UI state
   
   - Handle UI window lifecycle:
     - Window should run until closed or Ctrl+C
     - Sync should happen automatically in UI update loop

2. Update async client handling:
   - Ensure `AsyncLpClient` can be passed to UI
   - Handle transport sharing if needed

3. Test UI integration:
   - Verify UI spawns when dev command runs
   - Verify sync happens automatically
   - Verify UI displays project state

## Success Criteria

- UI spawns when dev command runs (when not headless)
- UI displays project state
- Sync happens automatically
- UI can be closed properly
- Code compiles without warnings

## Implementation Notes

- Use `eframe::run_native()` to spawn window
- Pass necessary state to `DebugUiState::new()`
- UI should run in the same thread as client (for simplicity)
- Handle window close events appropriately

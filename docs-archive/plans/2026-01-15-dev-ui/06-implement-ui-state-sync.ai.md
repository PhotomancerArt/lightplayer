# Phase 6: Implement UI state and sync logic

## Goal

Implement `DebugUiState` struct and sync logic for the debug UI.

## Tasks

1. Update `lp-app/apps/lp-cli/src/debug_ui/ui.rs`:
   - Implement `DebugUiState` struct:
     ```rust
     pub struct DebugUiState {
         project_view: Arc<Mutex<ClientProjectView>>,
         project_handle: ProjectHandle,
         async_client: AsyncLpClient,
         tracked_nodes: BTreeSet<NodeHandle>,
         all_detail: bool,
         sync_in_progress: bool,
         glsl_cache: BTreeMap<NodeHandle, String>,
     }
     ```
   - Implement `new()` method to create initial state
   - Implement `update()` method for egui App trait:
     - Check if sync is in progress
     - If not, start async sync task
     - Set sync_in_progress flag
     - Handle sync completion
   - Implement basic `update()` for `eframe::App`:
     - Call sync logic
     - Render basic UI (placeholder for now)

2. Add necessary imports:
   - `ClientProjectView` from `lp_engine_client`
   - `AsyncLpClient` from local module
   - `ProjectHandle` from `lp_model`
   - Standard library types (Arc, Mutex, BTreeSet, BTreeMap)

3. Handle async sync:
   - Use tokio spawn or similar for async task
   - Update sync_in_progress flag appropriately
   - Handle errors gracefully

## Success Criteria

- `DebugUiState` struct exists with all fields
- Sync logic works correctly
- No more than one sync in flight at a time
- Code compiles without warnings
- Basic UI structure is in place

## Implementation Notes

- Use `Arc<Mutex<ClientProjectView>>` for shared access
- Sync should happen as soon as previous sync completes
- Track sync state with boolean flag
- UI will read from view on each frame
